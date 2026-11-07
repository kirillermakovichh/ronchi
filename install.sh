#!/bin/sh
# Install a verified Ronchi release and its private, pinned Surfpool companion.
set -eu
LC_ALL=C
export LC_ALL
umask 022
fail() { printf 'ronchi install: %s\n' "$*" >&2; exit 1; }
usage() {
    printf '%s\n' 'Usage: sh install.sh [--version VERSION] [--prefix DIRECTORY]' \
        'Default prefix: $HOME/.local. Update by running this installer again.'
}
surfpool_version=1.7.0
version=${RONCHI_VERSION:-}
prefix=${RONCHI_INSTALL_PREFIX:-${HOME:?HOME must be set}/.local}
base=${RONCHI_RELEASE_BASE_URL:-https://github.com/kirillermakovichh/ronchi/releases}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) [ "$#" -ge 2 ] || fail '--version needs a value'; version=$2; shift 2 ;;
        --prefix) [ "$#" -ge 2 ] || fail '--prefix needs a value'; prefix=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done
case "$prefix" in /*) ;; *) fail 'Install prefix must be an absolute path' ;; esac
case "$base" in
    https://*) protocols='=https' ;;
    http://127.0.0.1:*|http://localhost:*|http://\[::1\]:*) protocols='=http,https' ;;
    *) fail 'Release URL must use HTTPS (HTTP is allowed only for explicit loopback mirrors)' ;;
esac
case "$(uname -s):$(uname -m)" in
    Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
    Darwin:x86_64) target=x86_64-apple-darwin ;;
    Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
    *) fail 'Supported targets are macOS arm64/x86_64 and Linux x86_64' ;;
esac
for command in curl tar mktemp awk grep sed cmp chmod ln mv mkdir rm rmdir sort uniq wc tr cat uname readlink; do
    command -v "$command" >/dev/null 2>&1 || fail "Missing required command: $command"
done
if command -v sha256sum >/dev/null 2>&1; then
    digest() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
    digest() { shasum -a 256 "$1" | awk '{print $1}'; }
else
    fail 'sha256sum or shasum is required'
fi
# TLS verification stays enabled. Limit redirects to the selected protocol set.
download() {
    curl --fail --silent --show-error --location --proto "$protocols" \
        --proto-redir "$protocols" --connect-timeout 20 --max-time 300 \
        --max-filesize 268435456 --output "$2" "$1" || fail "Download failed: $1"
}
mkdir -p "$prefix"
prefix=$(CDPATH= cd -P "$prefix" && pwd)
lock=$prefix/.ronchi-install-lock
mkdir "$lock" 2>/dev/null || fail "Another installation is running (lock: $lock)"
stage=
link=
cleanup() {
    [ -z "$link" ] || rm -f "$link"
    [ -z "$stage" ] || rm -rf "$stage"
    rmdir "$lock" 2>/dev/null || :
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
stage=$(mktemp -d "$prefix/.ronchi-install.XXXXXXXX")
base=${base%/}
if [ -z "$version" ]; then
    download "$base/latest/download/release-version.txt" "$stage/version"
    version=$(cat "$stage/version")
fi
version=${version#v}
printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' || fail 'Invalid release version'
[ "${#version}" -le 64 ] || fail 'Release version is too long'
name=ronchi-$version-$target
archive=$name.tar.gz
url=$base/download/v$version
download "$url/$archive.sha256" "$stage/checksum"
expected=$(cat "$stage/checksum")
printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || fail 'Invalid SHA-256 file'
printf 'Installing Ronchi %s with Surfpool for %s…\n' "$version" "$target"
download "$url/$archive" "$stage/archive.tar.gz"
[ "$(digest "$stage/archive.tar.gz")" = "$expected" ] || fail 'Archive SHA-256 mismatch; existing installation unchanged'
# Only fixed package paths and regular files/directories are extracted.
tar -tzf "$stage/archive.tar.gz" > "$stage/entries" || fail 'Invalid archive'
while IFS= read -r entry; do
    case "$entry" in
        "$name/"|"$name/bin/"|"$name/libexec/"|"$name/libexec/surfpool/"|"$name/libexec/surfpool/$surfpool_version/"|"$name/licenses/"|\
        "$name/bin/ronchi"|"$name/libexec/surfpool/$surfpool_version/surfpool"|"$name/libexec/surfpool/$surfpool_version/artifact.tar.gz"|\
        "$name/LICENSE"|"$name/release.json"|"$name/licenses/SURFPOOL-LICENSE"|\
        "$name/licenses/THIRD-PARTY-LICENSES.html"|"$name/licenses/SURFPOOL-THIRD-PARTY-LICENSES.html"|"$name/licenses/SURFPOOL-SOURCES.tar.gz") ;;
        *) fail "Unexpected archive entry: $entry" ;;
    esac
done < "$stage/entries"
[ "$(sort "$stage/entries" | uniq -d | wc -l | tr -d ' ')" = 0 ] || fail 'Duplicate archive entries'
tar -tvzf "$stage/archive.tar.gz" > "$stage/types" || fail 'Cannot inspect archive'
awk 'substr($0,1,1) != "-" && substr($0,1,1) != "d" {exit 1}' "$stage/types" || fail 'Archive links and special files are not allowed'
tar -xzf "$stage/archive.tar.gz" -C "$stage" || fail 'Extraction failed'
bundle=$stage/$name
for file in bin/ronchi libexec/surfpool/$surfpool_version/surfpool libexec/surfpool/$surfpool_version/artifact.tar.gz LICENSE release.json licenses/SURFPOOL-LICENSE licenses/THIRD-PARTY-LICENSES.html licenses/SURFPOOL-THIRD-PARTY-LICENSES.html licenses/SURFPOOL-SOURCES.tar.gz; do
    [ -s "$bundle/$file" ] && [ ! -L "$bundle/$file" ] || fail "Missing regular package file: $file"
done
chmod 755 "$bundle/bin/ronchi" "$bundle/libexec/surfpool/$surfpool_version/surfpool"
[ "$("$bundle/bin/ronchi" --version)" = "ronchi $version" ] || fail 'Ronchi executable cannot run or has an incorrect version'
[ "$("$bundle/libexec/surfpool/$surfpool_version/surfpool" --version)" = "surfpool $surfpool_version" ] || fail 'Surfpool executable cannot run or has an incorrect version'
releases=$prefix/lib/ronchi/releases
for directory in "$prefix/lib" "$prefix/lib/ronchi" "$releases" "$prefix/bin"; do
    [ ! -L "$directory" ] || fail "Install directory must not be a symlink: $directory"
    mkdir -p "$directory"
done
[ ! -L "$prefix/bin" ] || fail 'Install bin directory must not be a symlink'
[ ! -e "$prefix/bin/ronchi" ] || [ -L "$prefix/bin/ronchi" ] || fail 'Refusing to replace an unmanaged executable'
if [ -L "$prefix/bin/ronchi" ]; then
    case "$(readlink "$prefix/bin/ronchi")" in
        "$releases"/*/bin/ronchi) ;;
        *) fail 'Refusing to replace a ronchi link managed by another installer' ;;
    esac
fi
# A content-derived suffix permits rebuilds of the same version without mutating
# an executable that may still own a running Lab.
installation=$releases/$name-$(printf '%.16s' "$expected")
if [ -e "$installation" ] || [ -L "$installation" ]; then
    [ -d "$installation" ] && [ ! -L "$installation" ] || fail 'Invalid existing release directory'
    for directory in bin libexec libexec/surfpool libexec/surfpool/$surfpool_version licenses; do
        [ -d "$installation/$directory" ] && [ ! -L "$installation/$directory" ] || fail 'Existing release directories are damaged'
    done
    [ -x "$installation/bin/ronchi" ] && [ -x "$installation/libexec/surfpool/$surfpool_version/surfpool" ] || fail 'Existing release executables are damaged'
    while IFS= read -r entry; do
        relative=${entry#"$name/"}
        case "$relative" in ''|*/) continue ;; esac
        [ -f "$installation/$relative" ] && [ ! -L "$installation/$relative" ] && cmp -s "$bundle/$relative" "$installation/$relative" || fail 'Existing release is damaged; it was not overwritten'
    done < "$stage/entries"
else
    mv "$bundle" "$installation"
fi
link=$prefix/bin/.ronchi-link.$$
ln -s "$installation/bin/ronchi" "$link"
# Rename the link as a file, including when an existing link targets a directory.
if [ -L "$prefix/bin/ronchi" ] && [ -d "$prefix/bin/ronchi" ]; then fail 'Existing ronchi link targets a directory'; fi
mv -f "$link" "$prefix/bin/ronchi"
link=
printf 'Installed: %s/bin/ronchi\n' "$prefix"
case ":${PATH:-}:" in
    *":$prefix/bin:"*) printf 'Run: ronchi up\n' ;;
    *)
        printf '\nAdd this directory to PATH in your shell profile:\n'
        escaped=$(printf '%s' "$prefix/bin" | sed "s/'/'\\\\''/g")
        printf "  export PATH='%s':\"\$PATH\"\n" "$escaped"
        printf '\nThen run: ronchi up\n'
        ;;
esac
printf 'Surfpool is installed privately. Lab data stays in each project\047s ronchi/ directory.\n'
