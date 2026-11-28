#!/usr/bin/env python3
"""Copies the builders that the harness compares into namespaces of their own.

Usage: setup.py <out> [main revision] [#212 revision]

Writes <out>/<namespace>/*.hpp for each builder that isn't this branch's:
tafla_main (main's), tafla_212 (#212's), tafla_buf2 (this branch's, an
identical copy in another namespace) and tafla_raw (this branch's, with its
buffer allocated by `std::make_unique_for_overwrite()` rather than held in a
`std::vector`, and handed over as `Bytes`). The harness includes this
branch's own headers as `tafla`.
"""

import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
HEADERS = "cpp/include/tafla"

RAW_BUFFER = '''class TableBuffer {
public:
    static constexpr std::size_t kFirstCapacity = 256;
    static constexpr std::size_t kRoomForAField = 64;
    static constexpr std::size_t kMostRoomForFields = 64 << 10;
    static constexpr std::size_t kFourfoldBelow = 64 << 10;

    TableBuffer() noexcept = default;

    /// Room for `capacity` bytes, which the harness gives the raw variant
    /// where the others take a vector of that capacity
    explicit TableBuffer(std::size_t capacity)
        : owned_(std::make_unique_for_overwrite<std::uint8_t[]>(capacity)),
          data_(owned_.get()),
          capacity_(capacity) {}

    /// A vector's capacity, which the harness doesn't give this variant
    explicit TableBuffer(std::vector<std::uint8_t>&& vector) : TableBuffer(vector.capacity()) {}

    explicit TableBuffer(std::span<std::uint8_t> bytes) noexcept
        : data_(bytes.data()), capacity_(bytes.size()), fixed_(true) {}

    TableBuffer(TableBuffer&& other) noexcept
        : owned_(std::move(other.owned_)),
          data_(std::exchange(other.data_, nullptr)),
          size_(std::exchange(other.size_, 0)),
          capacity_(std::exchange(other.capacity_, 0)),
          fixed_(std::exchange(other.fixed_, false)) {}

    TableBuffer& operator=(TableBuffer&& other) noexcept {
        if (this != &other) {
            owned_ = std::move(other.owned_);
            data_ = std::exchange(other.data_, nullptr);
            size_ = std::exchange(other.size_, 0);
            capacity_ = std::exchange(other.capacity_, 0);
            fixed_ = std::exchange(other.fixed_, false);
        }
        return *this;
    }

    TableBuffer(const TableBuffer&) = delete;
    TableBuffer& operator=(const TableBuffer&) = delete;
    ~TableBuffer() = default;

    [[nodiscard]] std::uint8_t* data() noexcept {
        return data_;
    }

    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return data_;
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] static std::size_t first_capacity(std::size_t header, std::size_t fields) noexcept {
        const auto room =
            static_cast<std::size_t>((std::min<std::uint64_t>)(saturating_product(fields, kRoomForAField),
                                                               kMostRoomForFields));
        const auto capacity = header > SIZE_MAX - room ? SIZE_MAX : header + room;
        return (std::max)(kFirstCapacity, capacity);
    }

    void start(std::size_t header, std::size_t capacity) {
        if (fixed_) {
            if (header > capacity_) {
                throw_too_short(header, capacity_);
            }
        } else if (capacity > capacity_) {
            replace(capacity, 0);
        }
        size_ = 0;
    }

    TAFLA_ALWAYS_INLINE std::uint8_t* extend(std::size_t count) {
        if (count > capacity_ - size_) [[unlikely]] {
            grow(count);
        }
        std::uint8_t* at = data_ + size_;
        size_ += count;
        return at;
    }

    TAFLA_ALWAYS_INLINE void reserve_more(std::size_t count) {
        if (count > capacity_ - size_) [[unlikely]] {
            grow(count);
        }
    }

    void reserve_likely(std::size_t count) {
        if (!fixed_) {
            reserve_more(count);
        }
    }

    void truncate(std::size_t size) noexcept {
        size_ = size;
    }

    [[nodiscard]] Bytes release() {
        auto released = fixed_ ? Bytes::copy_of({data_, size_}) : Bytes::adopt(std::move(owned_), size_);
        *this = TableBuffer();
        return released;
    }

private:
    TAFLA_COLD void grow(std::size_t count) {
        const auto needed = count > SIZE_MAX - size_ ? SIZE_MAX : size_ + count;
        if (fixed_) {
            throw_too_short(needed, capacity_);
        }

        const std::size_t factor = capacity_ < kFourfoldBelow ? 4 : 2;
        const auto grown = capacity_ > SIZE_MAX / factor ? SIZE_MAX : factor * capacity_;
        const std::size_t most = PTRDIFF_MAX;
        if (needed > most) {
            throw std::bad_alloc();
        }
        replace((std::max)({needed, (std::min)(grown, most), kFirstCapacity}), size_);
    }

    void replace(std::size_t capacity, std::size_t keep) {
        auto grown = std::make_unique_for_overwrite<std::uint8_t[]>(capacity);
        copy(grown.get(), data_, keep);
        owned_ = std::move(grown);
        data_ = owned_.get();
        capacity_ = capacity;
    }

    std::unique_ptr<std::uint8_t[]> owned_;
    std::uint8_t* data_ = nullptr;
    std::size_t size_ = 0;
    std::size_t capacity_ = 0;
    bool fixed_ = false;
};
'''

ADOPT = '''    /// The first `size` of the bytes that `data` owns, which may be more
    static Bytes adopt(std::unique_ptr<std::uint8_t[]> data, std::size_t size) noexcept {
        Bytes bytes;
        bytes.data_ = std::move(data);
        bytes.size_ = size;
        return bytes;
    }

    Bytes(Bytes&& other) noexcept'''

WITH_CAPACITY = '''    /// Starts a table as the constructor does, in a buffer of `capacity`
    template <bool Versioned>
    [[nodiscard]] static TableBuilder with_capacity(std::size_t capacity, std::size_t fields,
                                                    TableLayout<Versioned> layout) {
        return TableBuilder(detail::TableBuffer(capacity), fields, layout);
    }

    /// Starts a table as the constructor does, in `bytes`, the caller's,'''


def headers_at(revision):
    names = subprocess.run(
        ["git", "-C", str(ROOT), "ls-tree", "--name-only", f"{revision}:{HEADERS}"],
        check=True, capture_output=True, text=True,
    ).stdout.split()
    return {
        name: subprocess.run(
            ["git", "-C", str(ROOT), "show", f"{revision}:{HEADERS}/{name}"],
            check=True, capture_output=True, text=True,
        ).stdout
        for name in names
    }


def working_headers():
    return {path.name: path.read_text() for path in (ROOT / HEADERS).glob("*.hpp")}


def renamed(text, namespace):
    suffix = namespace.upper()
    text = re.sub(r"\bnamespace tafla\b", f"namespace {namespace}", text)
    text = re.sub(r"\btafla::", f"{namespace}::", text)
    text = re.sub(r"\bTAFLA_([A-Z0-9_]*_HPP)\b", rf"{suffix}_\1", text)
    return text


def raw(headers):
    builder = headers["table_builder.hpp"]
    start = builder.index("class TableBuffer {")
    end = builder.index("};\n", start) + 3
    builder = builder[:start] + RAW_BUFFER + builder[end:]
    builder = builder.replace(
        "[[nodiscard]] std::vector<std::uint8_t> into_buffer() && {",
        "[[nodiscard]] Bytes into_buffer() && {",
    )
    marker = "    /// Starts a table as the constructor does, in `bytes`, the caller's,"
    assert marker in builder
    builder = builder.replace(marker, WITH_CAPACITY, 1)
    builder = builder.replace("#include <initializer_list>\n", "#include <initializer_list>\n#include <memory>\n", 1)

    bytes = headers["bytes.hpp"]
    assert "    Bytes(Bytes&& other) noexcept" in bytes
    bytes = bytes.replace("    Bytes(Bytes&& other) noexcept", ADOPT, 1)
    return {**headers, "table_builder.hpp": builder, "bytes.hpp": bytes}


def write(out, namespace, headers):
    directory = out / namespace
    directory.mkdir(parents=True, exist_ok=True)
    for name, text in headers.items():
        (directory / name).write_text(renamed(text, namespace))


def main():
    out = pathlib.Path(sys.argv[1]).resolve()
    main_revision = sys.argv[2] if len(sys.argv) > 2 else "630f0016"
    rewrite_revision = sys.argv[3] if len(sys.argv) > 3 else "59d1c2d7"

    write(out, "tafla_main", headers_at(main_revision))
    write(out, "tafla_212", headers_at(rewrite_revision))
    write(out, "tafla_buf2", working_headers())
    write(out, "tafla_raw", raw(working_headers()))


if __name__ == "__main__":
    main()
