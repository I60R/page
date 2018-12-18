# Maintainer: 160R@protonmail.com
_pkgname=page
pkgname=${_pkgname}-git
pkgrel=1
pkgver=v0.12.3.r58.g9403543
pkgdesc='pager that utilizes nvim terminal buffer'
arch=('i686' 'x86_64')
url="https://github.com/I60R/page"
license=('MIT')
depends=('neovim')
makedepends=('rust' 'cargo' 'git')
provides=('page')
conflicts=('page')
source=("git+https://github.com/I60R/page.git")
md5sums=('SKIP')

rootdir=$(cd "$srcdir")

pkgver() {
    checkout_project_root
    git describe --long --tags | sed 's/\([^-]*-g\)/r\1/;s/-/./g'
}

package() {
    checkout_project_root
    cargo build --release

    # Install binaries
    install -D -m755 "$PWD/target/release/page" "$pkgdir/usr/bin/page"
    install -D -m755 "$PWD/target/release/page-term-agent" "$pkgdir/usr/bin/page-term-agent"

    # Install shell completions
    install -D -m644 "$PWD/target/release/shell_completions/_page" "$pkgdir/usr/share/zsh/site-functions/_page"
    install -D -m644 "$PWD/target/release/shell_completions/page.bash" "$pkgdir/usr/share/bash-completion/completions/page"
    install -D -m644 "$PWD/target/release/shell_completions/page.fish" "$pkgdir/usr/share/fish/completions/page.fish"
}

# Ensures that current directory is root of repository
checkout_project_root() {
    cd "$srcdir"
    cd "$_pkgname" > /dev/null 2>&1 || cd ..
}