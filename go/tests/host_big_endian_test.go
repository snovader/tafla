//go:build !(386 || amd64 || arm || arm64 || loong64 || mips64le || mipsle || ppc64le || riscv64 || wasm) || tafla_portable

package tafla_test

// readsInPlace says whether the runtime should read aligned arrays in place:
// not on a big-endian host, nor with the build tag tafla_portable, which
// runs the path that converts every element
const readsInPlace = false
