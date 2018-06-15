# Maintainer: 160R@protonmail.com
_pkgname=page
pkgname=${_pkgname}-git
pkgrel=1
pkgver=v0.12.3.r12.g38d680e
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

pkgver() {
    cd "$srcdir"
    cd "$_pkgname" > /dev/null 2>&1 || cd ..
    git describe --long --tags | sed 's/\([^-]*-g\)/r\1/;s/-/./g'
}

package() {
    cd "$srcdir"
    cd "$_pkgname" > /dev/null 2>&1 || cd ..
    cargo install --force --bins --root $pkgdir/usr
    if [[ -f $pkgdir/usr/.crates.toml ]]; then
        rm $pkgdir/usr/.crates.toml
    fi
}
