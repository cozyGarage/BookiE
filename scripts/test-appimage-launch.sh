#!/usr/bin/env bash
set -euo pipefail
image_file="${1:?usage: test-appimage-launch.sh <BookiE-X.Y.Z-x86_64.AppImage> [container image...]}"
shift
images=("$@")
[ "${#images[@]}" -gt 0 ] || images=(ubuntu:26.04 fedora:latest archlinux:latest)
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/extract"
(cd "$work/extract" && "$(realpath "$image_file")" --appimage-extract >/dev/null)
cat > "$work/run.sh" <<'INNER'
#!/usr/bin/env bash
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq >/dev/null 2>&1
  apt-get install -y -qq xvfb dbus-x11 libx11-6 libx11-xcb1 libxcb1 libgl1 libegl1 libdrm2 libgbm1 \
    libfontconfig1 libfreetype6 libharfbuzz0b libfribidi0 libwayland-client0 >/dev/null 2>&1
elif command -v pacman >/dev/null; then
  pacman -Syu --noconfirm --needed xorg-server-xvfb dbus libx11 libxcb mesa libdrm fontconfig freetype2 harfbuzz \
    fribidi wayland xorg-xauth >/dev/null 2>&1
else
  dnf -q -y install xorg-x11-server-Xvfb dbus-x11 libX11 libX11-xcb libxcb mesa-libGL mesa-libEGL libdrm \
    mesa-libgbm fontconfig freetype harfbuzz fribidi libwayland-client >/dev/null 2>&1
fi
. /etc/os-release
mkdir -p /tmp/profile && export HOME=/tmp/profile XDG_CONFIG_HOME=/tmp/profile/c XDG_DATA_HOME=/tmp/profile/d \
  XDG_CACHE_HOME=/tmp/profile/k XDG_RUNTIME_DIR=/tmp/profile/r
mkdir -m 700 -p "$XDG_RUNTIME_DIR"
export GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none
log="$(timeout 20 xvfb-run -a dbus-run-session -- /app/squashfs-root/AppRun 2>&1 || true)"
if grep -q "main window activated" <<<"$log"; then
  echo "PASS $PRETTY_NAME"
else
  echo "FAIL $PRETTY_NAME"
  tail -5 <<<"$log"
  exit 1
fi
INNER
chmod +x "$work/run.sh"
status=0
for image in "${images[@]}"; do
  docker run --rm -v "$work/extract:/app:ro" -v "$work/run.sh:/run.sh:ro" "$image" /run.sh || status=1
done
exit "$status"
