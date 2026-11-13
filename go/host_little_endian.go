//go:build (386 || amd64 || arm || arm64 || loong64 || mips64le || mipsle || ppc64le || riscv64 || wasm) && !tafla_portable

package tafla

// hostLittleEndian says whether the host stores numbers little-endian, as
// tables do, so that an array's bytes are its elements in place. The
// architectures are those of encoding/binary's NativeEndian.
const hostLittleEndian = true
