# Maintainer: 160R@protonmail.com
_pkgname=page
pkgname=${_pkgname}-git
pkgrel=1
pkgver=git
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
    cd "$srcdir/$_pkgname"
    git describe --long --tags | sed 's/\([^-]*-g\)/r\1/;s/-/./g'
}

package() {
    cd "$srcdir/$_pkgname"
    cargo install --force --bins --root $pkgdir/usr
    if [[ -f $pkgdir/usr/.crates.toml ]]; then
        rm $pkgdir/usr/.crates.toml
    fi
}
