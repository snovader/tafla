//go:build !(386 || amd64 || arm || arm64 || loong64 || mips64le || mipsle || ppc64le || riscv64 || wasm) || tafla_portable

package tafla

// hostLittleEndian says whether the host stores numbers little-endian, as
// tables do, so that an array's bytes are its elements in place. It's false
// on big-endian hosts, and with the build tag tafla_portable, which runs the
// path that converts every element on any host, so that its tests run on a
// little-endian host too.
const hostLittleEndian = false
