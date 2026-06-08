#!/usr/bin/env bash
set -euo pipefail

distro="${PACKAGE_DISTRO:?PACKAGE_DISTRO is required}"
package_arch="${PACKAGE_ARCH:?PACKAGE_ARCH is required}"
artifact_arch="${ARTIFACT_ARCH:-$package_arch}"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"
release="1"
out_dir="artifacts/packages/${distro}-${artifact_arch}"
pkg_root="$(mktemp -d)"
work_dir="$(mktemp -d)"

trap 'rm -rf "$pkg_root" "$work_dir"' EXIT

export RUSTUP_HOME="${RUSTUP_HOME:-/tmp/rustup}"
export CARGO_HOME="${CARGO_HOME:-/tmp/cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/kernel-builder-target}"
export PATH="${CARGO_HOME}/bin:${PATH}"

install_deps() {
    case "$distro" in
        ubuntu)
            export DEBIAN_FRONTEND=noninteractive
            apt-get update
            apt-get install -y --no-install-recommends \
                ca-certificates curl build-essential pkg-config dpkg-dev xz-utils zstd file
            ;;
        fedora)
            dnf -y --setopt=install_weak_deps=False install \
                ca-certificates curl gcc make pkgconf-pkg-config rpm-build tar gzip xz zstd findutils file
            ;;
        arch)
            if ! command -v curl >/dev/null 2>&1 ||
                ! command -v cc >/dev/null 2>&1 ||
                ! command -v make >/dev/null 2>&1 ||
                ! command -v zstd >/dev/null 2>&1 ||
                ! command -v file >/dev/null 2>&1 ||
                ! command -v bsdtar >/dev/null 2>&1; then
                pacman -Sy --noconfirm --needed --overwrite '*' \
                    ca-certificates curl base-devel zstd xz file libarchive
            fi
            ;;
        *)
            echo "unsupported PACKAGE_DISTRO=$distro" >&2
            exit 2
            ;;
    esac
}

install_rust() {
    if ! command -v cargo >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
            sh -s -- -y --profile minimal --default-toolchain stable
    fi
    rustup default stable
    rustc --version
    cargo --version
}

stage_files() {
    cargo build --release --locked --bin kbs
    install -Dm0755 "${CARGO_TARGET_DIR}/release/kbs" "$pkg_root/usr/bin/kbs"
    strip "$pkg_root/usr/bin/kbs" || true
    install -Dm0644 config/example.toml "$pkg_root/etc/kernel-build-scheduler/config.toml"
    install -Dm0644 packaging/kernel-builder.service "$pkg_root/usr/lib/systemd/system/kernel-builder.service"
    install -Dm0644 packaging/kernel-builder-user.service "$pkg_root/usr/lib/systemd/user/kernel-builder-user.service"
    install -Dm0644 README.md "$pkg_root/usr/share/doc/kernel-builder/README.md"
}

build_deb() {
    local installed_size
    installed_size="$(du -sk "$pkg_root" | awk '{print $1}')"
    mkdir -p "$pkg_root/DEBIAN"
    cat >"$pkg_root/DEBIAN/control" <<EOF
Package: kernel-builder
Version: ${version}-${release}
Section: utils
Priority: optional
Architecture: ${package_arch}
Maintainer: Kernel Builder Maintainers <noreply@example.invalid>
Installed-Size: ${installed_size}
Depends: libc6
Description: Single-machine Linux kernel build scheduler
 Kernel Builder provides a local daemon, SQLite queue, CLI administration,
 container workers, and an MCP stdio adapter for scheduling Linux kernel builds.
EOF
    cat >"$pkg_root/DEBIAN/conffiles" <<EOF
/etc/kernel-build-scheduler/config.toml
EOF
    dpkg-deb --build --root-owner-group "$pkg_root" \
        "$out_dir/kernel-builder_${version}-${release}_${package_arch}.deb"
}

build_rpm() {
    local top source_dir spec changelog_date
    top="$work_dir/rpmbuild"
    source_dir="$work_dir/kernel-builder-${version}"
    spec="$top/SPECS/kernel-builder.spec"
    changelog_date="$(LC_ALL=C date -u '+%a %b %d %Y')"
    mkdir -p "$top/BUILD" "$top/RPMS" "$top/SOURCES" "$top/SPECS" "$top/SRPMS"
    mkdir -p "$source_dir"
    cp -a "$pkg_root"/. "$source_dir"/
    tar -C "$work_dir" -czf "$top/SOURCES/kernel-builder-${version}.tar.gz" "kernel-builder-${version}"
    cat >"$spec" <<EOF
%global debug_package %{nil}

Name: kernel-builder
Version: ${version}
Release: ${release}%{?dist}
Summary: Single-machine Linux kernel build scheduler
License: NOASSERTION
URL: https://github.com/sppidy/kernel-build-schd
Source0: %{name}-%{version}.tar.gz
Requires: glibc

%description
Kernel Builder provides a local daemon, SQLite queue, CLI administration,
container workers, and an MCP stdio adapter for scheduling Linux kernel builds.

%prep
%setup -q

%build

%install
mkdir -p %{buildroot}
cp -a . %{buildroot}/

%files
%attr(0755,root,root) /usr/bin/kbs
%config(noreplace) /etc/kernel-build-scheduler/config.toml
/usr/lib/systemd/system/kernel-builder.service
/usr/lib/systemd/user/kernel-builder-user.service
%doc /usr/share/doc/kernel-builder/README.md

%changelog
* ${changelog_date} Kernel Builder Maintainers <noreply@example.invalid> - ${version}-${release}
- Build CI package artifact
EOF
    rpmbuild -bb --define "_topdir $top" "$spec"
    find "$top/RPMS" -type f -name '*.rpm' -exec cp {} "$out_dir/" \;
}

build_arch() {
    local arch_root builddate size package
    arch_root="$work_dir/arch-root"
    package="$out_dir/kernel-builder-${version}-${release}-${package_arch}.pkg.tar.zst"
    mkdir -p "$arch_root"
    cp -a "$pkg_root"/. "$arch_root"/
    builddate="$(date +%s)"
    size="$(du -sb "$arch_root" | awk '{print $1}')"
    cat >"$arch_root/.PKGINFO" <<EOF
pkgname = kernel-builder
pkgbase = kernel-builder
pkgver = ${version}-${release}
pkgdesc = Single-machine Linux kernel build scheduler
url = https://github.com/sppidy/kernel-build-schd
builddate = ${builddate}
packager = GitHub Actions <actions@github.com>
size = ${size}
arch = ${package_arch}
license = custom
depend = glibc
backup = etc/kernel-build-scheduler/config.toml
EOF
    if command -v bsdtar >/dev/null 2>&1; then
        bsdtar --zstd -cf "$package" -C "$arch_root" .
    else
        tar --zstd -cf "$package" -C "$arch_root" .
    fi
}

mkdir -p "$out_dir"
rm -f "$out_dir"/*
install_deps
install_rust
stage_files

case "$distro" in
    ubuntu) build_deb ;;
    fedora) build_rpm ;;
    arch) build_arch ;;
esac

chmod -R a+rX "$out_dir"
find "$out_dir" -maxdepth 1 -type f -print -exec file {} \;
