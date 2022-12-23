pub use crate::{
    io_handler::{
        PipeOrSocketHandler,
        NotificationFromNeovim
    },
    io_pipe_or_socket::{
        PipeOrSocketWrite as IoWrite,
        PipeOrSocketRead as IoRead
    }
};
pub use nvim_rs::{
    neovim::Neovim,
    Buffer,
    Window,
    Value
};

use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt
};

use std::{
    path::Path,
    process::ExitStatus
};


pub fn init_logger() {
    let exec_time = std::time::Instant::now();

    let dispatch = fern::Dispatch::new().format(move |cb, msg, log_record| {
        let time = exec_time
            .elapsed()
            .as_micros();

        let lvl = log_record.level();
        let target = log_record.target();

        let mut module = log_record
            .module_path()
            .unwrap_or_default();
        let mut prep = " in ";
        if target == module {
            module = "";
            prep = "";
        };

        const BOLD: &str = "\x1B[1m";
        const UNDERL: &str = "\x1B[4m";
        const GRAY: &str = "\x1B[0;90m";
        const CLEAR: &str = "\x1B[0m";

        let mut msg_color = GRAY;
        if module.starts_with("page") {
            msg_color = "";
        };

        cb.finish(format_args!(
            "{BOLD}{UNDERL}[ {time:010} | {lvl:5} | \
            {target}{prep}{module} ]{CLEAR}\n{msg_color}{msg}{CLEAR}\n",
        ));
    });

    let log_lvl_filter = std::str::FromStr::from_str(
        std::env::var("PAGE_LOG")
            .as_deref()
            .unwrap_or("warn")
    ).expect("Cannot parse $PAGE_LOG value");

    dispatch
        .level(log_lvl_filter)
        .chain(std::io::stderr())
        // .chain(fern::log_file("page.log").unwrap())
        // .filter(|f| f.target() != "nvim_rs::neovim")
        .apply()
        .expect("Cannot initialize logger");
}


// If neovim dies unexpectedly it messes the terminal
// so terminal state must be cleaned
pub fn init_panic_hook() {
    let default_panic_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let try_spawn_reset = std::process::Command::new("reset")
            .spawn()
            .and_then(|mut child| child.wait());

        match try_spawn_reset {
            Ok(exit_code) if exit_code.success() => {}

            Ok(err_exit_code) => {
                log::error!(
                    target: "termreset",
                    "`reset` exited with status: {err_exit_code}"
                );
            }
            Err(e) => {
                log::error!(target: "termreset", "`reset` failed: {e:?}");
            }
        }

        default_panic_hook(panic_info);
    }));
}


/// This struct contains all neovim-related data which is
/// required by page after connection with neovim is established
pub struct NeovimConnection<Apis: From<Neovim<IoWrite>>> {
    pub nvim_proc: Option<tokio::task::JoinHandle<Result<ExitStatus, std::io::Error>>>,
    pub nvim_actions: Apis,
    pub initial_buf_number: i64,
    pub channel: u64,
    pub initial_win_and_buf: (Window<IoWrite>, Buffer<IoWrite>),
    pub rx: tokio::sync::mpsc::Receiver<NotificationFromNeovim>,
    handle: tokio::task::JoinHandle<Result<(), Box<nvim_rs::error::LoopError>>>,
}

/// Connects to parent neovim session or spawns
/// a new neovim process and connects to it through socket.
/// Replacement for `nvim_rs::Session::new_child()`,
/// since it uses --embed flag and steals page stdin
pub async fn open<Apis: From<Neovim<IoWrite>>>(
    tmp_dir: &Path,
    page_id: u128,
    nvim_listen_addr: &Option<String>,
    config_path: &Option<String>,
    custom_nvim_args: &Option<String>,
    print_protection: bool,
) -> NeovimConnection<Apis> {

    let (tx, rx) = tokio::sync::mpsc::channel(16);

    let handler = PipeOrSocketHandler {
        page_id: page_id.to_string(),
        tx
    };

    let mut nvim_proc = None;

    let (nvim, handle) = match nvim_listen_addr.as_deref() {
        Some(nvim_listen_addr)
            if nvim_listen_addr.parse::<std::net::SocketAddr>()
                .is_ok() =>
        {
            let tcp = tokio::net::TcpStream::connect(nvim_listen_addr)
                .await
                .expect("Cannot connect to neovim at TCP/IP address");

            let (rx, tx) = tokio::io::split(tcp);
            let (rx, tx) = (IoRead::Tcp(rx.compat()), IoWrite::Tcp(tx.compat_write()));
            let (nvim, io) = Neovim::<IoWrite>::new(rx, tx, handler);
            let io_handle = tokio::task::spawn(io);

            (nvim, io_handle)
        }

        Some(nvim_listen_addr) => {
            let ipc = parity_tokio_ipc::Endpoint::connect(nvim_listen_addr)
                .await
                .expect("Cannot connect to neovim at path");

            let (rx, tx) = tokio::io::split(ipc);
            let (rx, tx) = (IoRead::Ipc(rx.compat()), IoWrite::Ipc(tx.compat_write()));
            let (nvim, io) = Neovim::<IoWrite>::new(rx, tx, handler);
            let io_handle = tokio::task::spawn(io);

            (nvim, io_handle)
        }

        None => {
            let (nvim, io_handle, child) = create_new_neovim_process_ipc(
                tmp_dir,
                page_id,
                config_path,
                custom_nvim_args,
                print_protection,
                handler
            )
            .await;
            nvim_proc = Some(child);

            (nvim, io_handle)
        }
    };

    let channel = nvim
        .get_api_info()
        .await
        .expect("No API info")
        .get(0)
        .expect("No channel")
        .as_u64()
        .expect("Channel not a number");

    let initial_win = nvim
        .get_current_win()
        .await
        .expect("Cannot get initial window");

    let initial_buf = nvim
        .get_current_buf()
        .await
        .expect("Cannot get initial buffer");

    let initial_buf_number = initial_buf
        .get_number()
        .await
        .expect("Cannot get initial buffer number");

    NeovimConnection {
        nvim_proc,
        nvim_actions: From::from(nvim),
        initial_buf_number,
        channel,
        initial_win_and_buf: (initial_win, initial_buf),
        rx,
        handle
    }
}


/// Waits until child neovim closes.
/// If no child neovim process spawned then it's safe to just exit from page
pub async fn close_and_exit<Apis: From<Neovim<IoWrite>>>(
    nvim_connection: &mut NeovimConnection<Apis>
) -> ! {
    log::trace!(target: "exit", "close and exit");

    if let Some(ref mut process) = nvim_connection.nvim_proc {
        if !process.is_finished() {
            process
               .await
               .expect("Neovim process was spawned with error")
               .expect("Neovim process died unexpectedly");
        }
    }

    nvim_connection.handle
        .abort();

    log::logger()
        .flush();

    std::process::exit(0)
}


/// Creates a new session using UNIX socket.
/// Also prints protection from shell redirection
/// that could cause some harm (see --help[-W])
async fn create_new_neovim_process_ipc(
    tmp_dir: &Path,
    page_id: u128,
    config: &Option<String>,
    custom_args: &Option<String>,
    print_protection: bool,
    handler: PipeOrSocketHandler
) -> (
    Neovim<IoWrite>,
    tokio::task::JoinHandle<Result<(), Box<nvim_rs::error::LoopError>>>,
    tokio::task::JoinHandle<Result<ExitStatus, std::io::Error>>
) {
    if print_protection {
        print_redirect_protection(tmp_dir);
    }

    let nvim_listen_addr = tmp_dir
        .join(&format!("socket-{page_id}"));

    let mut nvim_proc = tokio::task::spawn({
        let (config, custom_args, nvim_listen_addr) = (
            config.clone(),
            custom_args.clone(),
            nvim_listen_addr.clone()
        );
        async move {
            spawn_child_nvim_process(
                &config,
                &custom_args,
                &nvim_listen_addr
            )
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(128)).await;

    let mut i = 0;
    let e = loop {

        let connection = parity_tokio_ipc::Endpoint::connect(&nvim_listen_addr).await;
        match connection {
            Ok(ipc) => {
                log::trace!(target: "child neovim spawned", "attempts={i}");

                let (rx, tx) = tokio::io::split(ipc);
                let (rx, tx) = (IoRead::Ipc(rx.compat()), IoWrite::Ipc(tx.compat_write()));
                let (neovim, io) = Neovim::<IoWrite>::new(rx, tx, handler);
                let io_handle = tokio::task::spawn(io);

                return (neovim, io_handle, nvim_proc)
            }

            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => {
                if i == 256 {
                    break e
                }

                use std::task::Poll::{Ready, Pending};
                let poll = futures::poll!(std::pin::Pin::new(&mut nvim_proc));

                match poll {
                    Ready(Err(join_e)) => {
                        log::error!(target: "child neovim didn't start", "{join_e}");

                        break join_e.into()
                    },
                    Ready(Ok(child)) => {
                        log::error!(target: "child neovim finished", "{child:?}");

                        break e
                    },

                    Pending => {},
                }

                tokio::time::sleep(std::time::Duration::from_millis(16)).await;

                i += 1;
            }

            Err(e) => break e
        }
    };

    panic!("Cannot connect to neovim: attempts={i}, address={nvim_listen_addr:?}, {e:?}");
}


/// This is hack to prevent behavior (or bug) in some shells (see --help[-W])
fn print_redirect_protection(tmp_dir: &Path) {
    let d = tmp_dir
        .join("DO-NOT-REDIRECT-OUTSIDE-OF-NVIM-TERM(--help[-W])");

    if let Err(e) = std::fs::create_dir_all(&d) {
        panic!("Cannot create protection directory '{}': {e:?}", d.display())
    }

    println!("{}", d.to_string_lossy());
}

/// Spawns child neovim process on top of page,
/// which further will be connected to page with UNIX socket.
/// In this way neovim UI is displayed properly on top of page,
/// and page as well is able to handle its own input to redirect it
/// unto proper target (which is impossible with methods provided by
/// `neovim_lib`). Also custom neovim config will be picked
/// if it exists on corresponding locations.
fn spawn_child_nvim_process(
    config: &Option<String>,
    custom_args: &Option<String>,
    nvim_listen_addr: &Path
) -> Result<ExitStatus, std::io::Error> {

    let nvim_args = {
        let mut a = String::new();
        a.push_str("--cmd 'set shortmess+=I' ");
        a.push_str("--listen ");
        a.push_str(&nvim_listen_addr.to_string_lossy());

        if let Some(config) = config
            .clone()
            .or_else(default_config_path)
        {
            a.push(' ');
            a.push_str("-u ");
            a.push_str(&config);
        }

        if let Some(custom_args) = custom_args.as_ref() {
            a.push(' ');
            a.push_str(custom_args);
        }

        shell_words::split(&a)
            .expect("Cannot parse neovim arguments")
    };

    log::trace!(target: "new neovim process", "Args: {nvim_args:?}");

    let term = current_term();

    std::process::Command::new("nvim")
        .args(&nvim_args)
        .stdin(term)
        .spawn()
        .expect("Cannot spawn a child neovim process")
        .wait()
}


fn current_term() -> std::fs::File {
    #[cfg(windows)]
    let dev = "CON:";
    #[cfg(not(windows))]
    let dev = "/dev/tty";

    std::fs::OpenOptions::new()
        .read(true)
        .open(dev)
        .expect("Cannot open current terminal device")
}


/// Returns path to custom neovim config if
/// it's present in a corresponding locations
fn default_config_path() -> Option<String> {
    use std::path::PathBuf;

    let page_home = std::env::var("XDG_CONFIG_HOME")
        .map(|xdg_config_home| {
            PathBuf::from(xdg_config_home)
                .join("page")
        });

    let page_home = page_home.or_else(|_| std::env::var("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join(".config/page")
        }));

    log::trace!(target: "config", "directory is: {page_home:?}");

    let Ok(page_home) = page_home else {
        return None;
    };

    let init_lua = page_home
        .join("init.lua");
    if init_lua.exists() {
        let p = init_lua.to_string_lossy().to_string();
        log::trace!(target: "config", "use init.lua");
        return Some(p)
    }

    let init_vim = page_home
        .join("init.vim");
    if init_vim.exists() {
        let p = init_vim.to_string_lossy().to_string();
        log::trace!(target: "config", "use init.vim");
        return Some(p)
    }

    None
}


mod io_pipe_or_socket {
    use parity_tokio_ipc::Connection;
    use tokio::{
        io::{ReadHalf, WriteHalf},
        net::TcpStream
    };
    use tokio_util::compat::Compat;
    use std::pin::Pin;

    pub enum PipeOrSocketRead {
        Ipc(Compat<ReadHalf<Connection>>),
        Tcp(Compat<ReadHalf<TcpStream>>),
    }

    pub enum PipeOrSocketWrite {
        Ipc(Compat<WriteHalf<Connection>>),
        Tcp(Compat<WriteHalf<TcpStream>>),
    }

    macro_rules! delegate {
        ($self:ident => $method:ident($($args:expr),*)) => {
            match $self.get_mut() {
                Self::Ipc(rw) => Pin::new(rw).$method($($args),*),
                Self::Tcp(rw) => Pin::new(rw).$method($($args),*),
            }
        };
    }

    impl futures::AsyncRead for PipeOrSocketRead {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut [u8]
        ) -> std::task::Poll<std::io::Result<usize>> {
            delegate!(self => poll_read(cx, buf))
        }
    }

    impl futures::AsyncWrite for PipeOrSocketWrite {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8]
        ) -> std::task::Poll<std::io::Result<usize>> {
            delegate!(self => poll_write(cx, buf))
        }


        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>
        ) -> std::task::Poll<std::io::Result<()>> {
            delegate!(self => poll_flush(cx))
        }


        fn poll_close(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>
        ) -> std::task::Poll<std::io::Result<()>> {
            delegate!(self => poll_close(cx))
        }
    }
}


mod io_handler {
    use super::{io_pipe_or_socket::PipeOrSocketWrite, Neovim, Value};

    /// Receives and collects notifications from neovim side over IPC or TCP/IP
    #[derive(Clone)]
    pub struct PipeOrSocketHandler {
        pub tx: tokio::sync::mpsc::Sender<NotificationFromNeovim>,
        pub page_id: String,
    }

    #[async_trait::async_trait]
    impl nvim_rs::Handler for PipeOrSocketHandler {
        type Writer = PipeOrSocketWrite;

        async fn handle_request(
            &self,
            request: String,
            args: Vec<Value>,
            _: Neovim<PipeOrSocketWrite>
        ) -> Result<Value, Value> {
            log::warn!(target: "unhandled", "{request}: {args:?}");

            Ok(Value::from(0))
        }

        async fn handle_notify(
            &self,
            notification: String,
            args: Vec<Value>,
            _: Neovim<PipeOrSocketWrite>
        ) {
            log::trace!(target: "notification", "{}: {:?} ", notification, args);

            let page_id = args
                .get(0)
                .and_then(Value::as_str);

            let same_page_id = page_id
                .map_or(false, |page_id| page_id == self.page_id);
            if !same_page_id {
                log::warn!(target: "invalid page id", "{page_id:?}");

                return
            }

            let notification_from_neovim = match notification.as_str() {
                "page_fetch_lines" => {
                    let count = args.get(1)
                        .and_then(Value::as_u64);

                    if let Some(lines_count) = count {
                        NotificationFromNeovim::FetchLines(lines_count as usize)
                    } else {
                        NotificationFromNeovim::FetchPart
                    }
                },
                "page_buffer_closed" => {
                    NotificationFromNeovim::BufferClosed
                },

                unknown => {
                    log::warn!(target: "unhandled notification", "{unknown}");

                    return
                }
            };

            self.tx
                .send(notification_from_neovim)
                .await
                .expect("Cannot receive notification");
        }
    }


    /// This enum represents all notifications
    /// that could be sent from page's commands on neovim side
    #[derive(Debug)]
    pub enum NotificationFromNeovim {
        FetchPart,
        FetchLines(usize),
        BufferClosed,
    }
}
