// Counts what each builder's call of each row allocates, over 20 calls after
// two: the bytes asked for and the allocations, per call, and the most bytes
// held at once during a call beyond those held before it. It replaces the
// global `operator new` and `operator delete`, which every builder allocates
// with, and measures what each allocation holds with `malloc_size()`. It
// prints each builder's size first. Arguments select rows by a part of their
// name.

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <new>
#include <string>
#include <vector>

#include <malloc/malloc.h>

#include "catalog.hpp"

namespace {

std::uint64_t allocations = 0;
std::uint64_t requested = 0;
std::uint64_t held = 0;
std::uint64_t most_held = 0;
bool counting = false;

void* allocate(std::size_t size) {
    // NOLINTNEXTLINE(cppcoreguidelines-no-malloc): this is the allocator
    void* pointer = std::malloc(size == 0 ? 1 : size);
    if (pointer == nullptr) {
        throw std::bad_alloc();
    }
    if (counting) {
        ++allocations;
        requested += size;
    }
    held += malloc_size(pointer);
    most_held = held > most_held ? held : most_held;
    return pointer;
}

void release(void* pointer) noexcept {
    if (pointer != nullptr) {
        held -= malloc_size(pointer);
        std::free(pointer);  // NOLINT(cppcoreguidelines-no-malloc)
    }
}

}  // namespace

void* operator new(std::size_t size) {
    return allocate(size);
}

void* operator new[](std::size_t size) {
    return allocate(size);
}

void operator delete(void* pointer) noexcept {
    release(pointer);
}

void operator delete[](void* pointer) noexcept {
    release(pointer);
}

void operator delete(void* pointer, std::size_t /*size*/) noexcept {
    release(pointer);
}

void operator delete[](void* pointer, std::size_t /*size*/) noexcept {
    release(pointer);
}

int main(int argc, char** argv) {
    constexpr std::size_t calls = 20;
    std::vector<std::string> filters(argv + 1, argv + argc);

    std::printf("# sizeof: main %zu, #212 %zu, buffer %zu, raw %zu; NestedTable: main %zu, #212 %zu, buffer %zu\n",
                sizeof(tafla_main::TableBuilder), sizeof(tafla_212::TableBuilder), sizeof(tafla::TableBuilder),
                sizeof(tafla_raw::TableBuilder), sizeof(tafla_main::NestedTable), sizeof(tafla_212::NestedTable),
                sizeof(tafla::NestedTable));
    std::printf("# row\tvariant\tbytes per call\tallocations per call\tmost held\n");

    const builder_rewrite::Inputs inputs;
    builder_rewrite::for_each_row(inputs, filters, [&](const builder_rewrite::Row& row) {
        for (const auto& variant : row.variants) {
            variant.loop(2);

            allocations = 0;
            requested = 0;
            std::uint64_t most = 0;
            counting = true;
            for (std::size_t call = 0; call < calls; ++call) {
                const auto before = held;
                most_held = held;
                variant.loop(1);
                most = most_held - before > most ? most_held - before : most;
            }
            counting = false;

            std::printf("%s\t%s\t%llu\t%.2f\t%llu\n", row.name.c_str(), variant.name.c_str(),
                        static_cast<unsigned long long>(requested / calls),
                        static_cast<double>(allocations) / static_cast<double>(calls),
                        static_cast<unsigned long long>(most));
        }
        std::fflush(stdout);
    });
    return 0;
}
