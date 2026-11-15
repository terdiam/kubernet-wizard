#!/usr/bin/env bash
# Builds .deb and .rpm installers (Debian/Ubuntu, RHEL/Fedora family).
# Usage: package-linux.sh <app-binary> <kw-binary> <version> <out-dir>
#
# Installs to /usr, so in-app auto-update hands updates to the package
# manager (kw-update treats /usr as package-managed).
set -euo pipefail
app_bin="$1"; kw_bin="$2"; version="$3"; out="$4"
root_dir="$(cd "$(dirname "$0")/.." && pwd)"
name=kubernet-wizard
summary="Design, generate and deploy Kubernetes clusters"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

# Shared file tree.
tree="$work/tree"
install -Dm755 "$app_bin" "$tree/usr/bin/$name"
install -Dm755 "$kw_bin" "$tree/usr/bin/kw"
install -Dm644 "$root_dir/assets/linux/$name.desktop" "$tree/usr/share/applications/$name.desktop"
for png in "$root_dir"/assets/icon/hicolor/*.png; do
  size="$(basename "$png" .png)"
  install -Dm644 "$png" "$tree/usr/share/icons/hicolor/$size/apps/$name.png"
done
install -Dm644 "$root_dir/LICENSE" "$tree/usr/share/doc/$name/copyright"

# Debian package names for the shared libraries the binaries link
# (dlopen'd ones such as Vulkan/Wayland go to Recommends). Computed from
# ELF NEEDED entries so it also works on non-Debian build hosts.
deb_depends() {
  local glibc soname pkg deps=()
  glibc="$(objdump -T "$@" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)"
  deps+=("libc6 (>= ${glibc:-2.17})")
  for soname in $(objdump -p "$@" | awk '/NEEDED/ {print $2}' | sort -u); do
    case "$soname" in
      libc.so.6|libm.so.6|libdl.so.2|libpthread.so.0|librt.so.1|ld-linux-*) continue ;;
      libgcc_s.so.1) pkg=libgcc-s1 ;;
      libxcb.so.1) pkg=libxcb1 ;;
      libxkbcommon.so.0) pkg=libxkbcommon0 ;;
      libxkbcommon-x11.so.0) pkg=libxkbcommon-x11-0 ;;
      libX11.so.6) pkg=libx11-6 ;;
      libX11-xcb.so.1) pkg=libx11-xcb1 ;;
      libfontconfig.so.1) pkg=libfontconfig1 ;;
      libfreetype.so.6) pkg=libfreetype6 ;;
      libzstd.so.1) pkg=libzstd1 ;;
      libasound.so.2) pkg="libasound2 | libasound2t64" ;;
      libssl.so.3|libcrypto.so.3) pkg="libssl3 | libssl3t64" ;;
      libwayland-client.so.0) pkg=libwayland-client0 ;;
      *) echo "package-linux.sh: no Debian package mapping for $soname" >&2; exit 1 ;;
    esac
    deps+=("$pkg")
  done
  printf '%s\n' "${deps[@]}" | awk '!seen[$0]++' | paste -sd, - | sed 's/,/, /g'
}

# ---------------------------------------------------------------- .deb
if command -v dpkg-deb >/dev/null; then
  deb="$work/deb"
  cp -a "$tree" "$deb"
  depends="$(deb_depends "$deb/usr/bin/$name" "$deb/usr/bin/kw")"
  arch="$(dpkg --print-architecture)"
  mkdir -p "$deb/DEBIAN"
  cat > "$deb/DEBIAN/control" <<EOF
Package: $name
Version: $version
Section: admin
Priority: optional
Architecture: $arch
Maintainer: kubernet-wizard maintainers <noreply@github.com>
Depends: $depends
Recommends: libvulkan1, libwayland-client0, libxkbcommon-x11-0, ansible, openssh-client
Suggests: opentofu, kubectl, helm
Homepage: https://github.com/terdiam/kubernet-wizard
Description: $summary
 Desktop wizard that renders OpenTofu + Ansible projects for kubeadm, k3s
 and RKE2 clusters and runs them with live logs.
EOF
  dpkg-deb --root-owner-group --build "$deb" "$out/${name}_${version}_${arch}.deb" >/dev/null
  echo "built $out/${name}_${version}_${arch}.deb"
fi

# ---------------------------------------------------------------- .rpm
if command -v rpmbuild >/dev/null; then
  rpm_version="$(printf '%s' "$version" | tr '-' '~')"  # rpm forbids '-' in Version
  top="$work/rpm"
  mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  cat > "$top/SPECS/$name.spec" <<EOF
Name:           $name
Version:        $rpm_version
Release:        1
Summary:        $summary
License:        Apache-2.0
URL:            https://github.com/terdiam/kubernet-wizard
Recommends:     ansible-core, openssh-clients, vulkan-loader
%global debug_package %{nil}
%global __strip /bin/true
%global _build_id_links none

%description
Desktop wizard that renders OpenTofu + Ansible projects for kubeadm, k3s
and RKE2 clusters and runs them with live logs.

%install
cp -a "$tree/." %{buildroot}/

%files
/usr/bin/$name
/usr/bin/kw
/usr/share/applications/$name.desktop
/usr/share/icons/hicolor/*/apps/$name.png
%license /usr/share/doc/$name/copyright
EOF
  rpmbuild --quiet --define "_topdir $top" -bb "$top/SPECS/$name.spec"
  find "$top/RPMS" -name '*.rpm' -exec cp {} "$out/" \;
  echo "built $(ls "$out"/*.rpm)"
fi
