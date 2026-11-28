#!/bin/sh
# Builds the harness into <out>, beside the copies that setup.py writes there:
#   sh cpp/benchmarks/builder_rewrite/build.sh <out>
# with every function aligned to 64 bytes, so that a function's placement
# doesn't depend on the code before it, as #371 found for Rust.
set -e
here=$(cd "$(dirname "$0")" && pwd)
out=$1
python3 "$here/setup.py" "$out"
for program in timing allocation; do
    c++ -std=c++20 -O3 -DNDEBUG -falign-functions=64 -I "$here/../../include" -I "$out" \
        "$here/$program.cpp" -o "$out/$program"
done
