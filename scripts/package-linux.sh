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

# ---------------------------------------------------------------- .deb
if command -v dpkg-deb >/dev/null; then
  deb="$work/deb"
  cp -a "$tree" "$deb"
  # Shared-library dependencies of the binaries (dlopen'd ones go to Recommends).
  depends="$(cd "$work" && mkdir -p debian && printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' "$name" "$name" > debian/control \
    && dpkg-shlibdeps -O --ignore-missing-info "$deb/usr/bin/$name" "$deb/usr/bin/kw" 2>/dev/null | sed -n 's/^shlibs:Depends=//p')"
  arch="$(dpkg --print-architecture)"
  mkdir -p "$deb/DEBIAN"
  cat > "$deb/DEBIAN/control" <<EOF
Package: $name
Version: $version
Section: admin
Priority: optional
Architecture: $arch
Maintainer: kubernet-wizard maintainers <noreply@github.com>
Depends: ${depends:-libc6}
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
