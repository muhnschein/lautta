#!/bin/sh
# SPDX-License-Identifier: LGPL-2.1-or-later
# Lifts what a host cargo needs to cross-build for the Sailfish OS target out
# of the Platform SDK image (SPEC RS-6, doc/CONTRIBUTING-CODE.md "Toolchain"):
# the SDK's aarch64 gcc and binutils (/opt/cross, i486 binaries), the host
# libraries they load from the image, and the target's sysroot. Then adds
# what the image leaves to scratchbox2: binutils under the names gcc looks
# for, compiler wrappers with the sysroot, and link names plus pkg-config
# files for the target libraries that ship without -devel packages (the
# binary still records libsqlite3.so.0 and libbz2.so.1).
# The host needs the i386 C library (Ubuntu: libc6-i386).
# Usage: lift-sdk.sh <image> <target name> <dir>
set -eu
image=$1
target=$2
dir=$3
triple=aarch64-meego-linux-gnu
rm -rf "$dir"
mkdir -p "$dir/hostlib" "$dir/bin" "$dir/linklib/pkgconfig"
dir=$(cd "$dir" && pwd)
cid=$(docker create "$image" /bin/true)
trap 'docker rm "$cid" > /dev/null' EXIT
# Through tar, so that an unprivileged user can unpack it: no device nodes,
# no ownership, and every file readable and removable afterwards.
lift() {
    from=$1
    to=$2
    mkdir -p "$to"
    docker cp "$cid:$from/." - | tar -x -C "$to" --no-same-owner --exclude='./dev/*'
    chmod -R u+rwX "$to"
}
lift /opt/cross "$dir/cross"
lift "/srv/mer/targets/$target" "$dir/sysroot"
for lib in libmpc.so.3 libmpfr.so.6 libgmp.so.10 libz.so.1 libzstd.so.1; do
    docker cp -L "$cid:/usr/lib/$lib" "$dir/hostlib/$lib"
done
# gcc runs as and ld from <prefix>/<triple>/bin; in the SDK sb2 maps them.
mkdir -p "$dir/cross/$triple/bin"
for tool in as ld ld.bfd ar nm objcopy objdump ranlib readelf strip; do
    if [ -e "$dir/cross/bin/$triple-$tool" ]; then
        ln -sf "../../bin/$triple-$tool" "$dir/cross/$triple/bin/$tool"
    fi
done
for tool in gcc g++ ar; do
    sysroot="--sysroot=$dir/sysroot -Wl,-rpath-link,$dir/sysroot/usr/lib64:$dir/sysroot/lib64"
    [ "$tool" != ar ] || sysroot=
    cat > "$dir/bin/$triple-$tool" <<WRAP
#!/bin/sh
LD_LIBRARY_PATH="$dir/hostlib" exec "$dir/cross/bin/$triple-$tool" $sysroot "\$@"
WRAP
    chmod +x "$dir/bin/$triple-$tool"
done
# Link names and pkg-config files for libraries whose -devel packages the
# target lacks; the crates ship their own bindings, so no headers are needed.
link() {
    lib=$1
    soname=$2
    pc=$3
    ln -sf "$dir/sysroot/usr/lib64/$soname" "$dir/linklib/$lib.so"
    cat > "$dir/linklib/pkgconfig/$pc.pc" <<PC
Name: $pc
Description: $soname of the Sailfish OS target
Version: 0
Libs: -L$dir/linklib -l${lib#lib}
Cflags:
PC
}
link libsqlite3 libsqlite3.so.0 sqlite3
link libbz2 libbz2.so.1 bzip2
"$dir/bin/$triple-gcc" --version | head -n 1
