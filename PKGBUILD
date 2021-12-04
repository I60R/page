# Maintainer: 160R@protonmail.com
_pkgname=page
pkgname=${_pkgname}-git
pkgrel=1
pkgver=v3.0.0
pkgdesc='Advanced, fast pager powered by neovim and inspired by neovim-remote'
arch=('i686' 'x86_64')
url="https://github.com/I60R/page"
license=('MIT')
depends=('neovim' 'gcc-libs')
makedepends=('rust' 'cargo' 'git')
provides=('page')
conflicts=('page')
source=("git+https://github.com/I60R/page.git")
md5sums=('SKIP')


pkgver() {
    checkout_project_root
    git describe --tags --abbrev=0
}

prepare() {
    rustc_version=$([[ "$(rustc --version)" =~ rustc\ 1.([0-9]+).* ]] && echo "${BASH_REMATCH[1]}")

    (($rustc_version >= 50)) && return 0;

    # Set error color
    echo -e '\e[0;31m'
    echo 'Minimum supported rust version is 1.40.0, please update'
    echo ' * rustup way: `rustup update`'
    echo ' * pacman way: `pacman -Sy rust`'
    # Reset color
    echo -e '\e[0m'
    return 1
}

package() {
    checkout_project_root

    cargo build --release

    # Install binaries
    install -D -m755 "target/release/page" "$pkgdir/usr/bin/page"

    # Find last build directory where completions was generated
    completions_dir=$(find "target" -name "shell_completions" -type d -printf "%T+\t%p\n" | sort | awk 'NR==1{print $2}')

    # Install shell completions
    install -D -m644 "$completions_dir/_page" "$pkgdir/usr/share/zsh/site-functions/_page"
    install -D -m644 "$completions_dir/page.bash" "$pkgdir/usr/share/bash-completion/completions/page"
    install -D -m644 "$completions_dir/page.fish" "$pkgdir/usr/share/fish/completions/page.fish"

    # Install MIT license
    install -D -m644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}

# Ensures that current directory is root of repository
checkout_project_root() {
    cd "$srcdir"
    cd "$_pkgname" > /dev/null 2>&1 || cd ..
}
