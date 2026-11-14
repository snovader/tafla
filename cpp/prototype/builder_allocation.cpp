// Counts what each builder row's call allocates through `operator new`, which
// `tafla::TableBuilder`, `Bytes` and the prototype allocate with: the
// allocations and the bytes they ask for, over 100 calls after one, divided
// among them, as `tafla_allocation` counts them.

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <new>

#include "builder_suite.hpp"

namespace {

std::uint64_t allocations = 0;
std::uint64_t allocated = 0;
bool counting = false;

}  // namespace

void* operator new(std::size_t size) {
    if (counting) {
        ++allocations;
        allocated += size;
    }
    // NOLINTNEXTLINE(cppcoreguidelines-no-malloc): this is the allocator
    if (void* pointer = std::malloc(size == 0 ? 1 : size)) {
        return pointer;
    }
    throw std::bad_alloc();
}

void* operator new[](std::size_t size) {
    return ::operator new(size);
}

void operator delete(void* pointer) noexcept {
    std::free(pointer);  // NOLINT(cppcoreguidelines-no-malloc)
}

void operator delete[](void* pointer) noexcept {
    ::operator delete(pointer);
}

void operator delete(void* pointer, std::size_t /*size*/) noexcept {
    ::operator delete(pointer);
}

void operator delete[](void* pointer, std::size_t /*size*/) noexcept {
    ::operator delete(pointer);
}

int main() {
    constexpr std::size_t calls = 100;
    const tafla::prototype::Inputs inputs;

    std::printf("Bytes allocated per call, and the allocations that ask for them\n");
    for (const auto& benchmark : tafla::prototype::benchmarks(inputs)) {
        std::printf("%s\n", benchmark.name.c_str());
        for (const auto& variant : benchmark.variants) {
            variant.loop(1);

            allocations = 0;
            allocated = 0;
            counting = true;
            variant.loop(calls);
            counting = false;

            std::printf("  %-18s %12llu (%llu)\n", variant.name.c_str(),
                        static_cast<unsigned long long>(allocated / calls),
                        static_cast<unsigned long long>(allocations / calls));
        }
    }
    return 0;
}
