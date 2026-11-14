// Counts a table's bytes, and writes them forward into bytes of that length.

#ifndef TAFLA_TABLE_WRITER_HPP
#define TAFLA_TABLE_WRITER_HPP

#include <array>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>
#include <ranges>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>

#include "bytes.hpp"
#include "element.hpp"
#include "errors.hpp"
#include "layout.hpp"
#include "structs.hpp"
#include "table_length.hpp"

namespace tafla {

template <bool Checked>
class BasicTableWriter;

/// The writer that `TableSizer::writer()` returns, which checks each write
using TableWriter = BasicTableWriter<true>;

/// A view of items that convert into `Item`, from which tafla-gen's generic
/// fields structs write a vector or an array of structs: a `std::span` of
/// `Item`s, or `std::views::transform()` of the caller's own items, say. It's
/// a view, so that a fields struct holds it rather than a copy of the items,
/// it knows its size, and it can be iterated more than once, since a fields
/// struct counts the items' bytes before it writes them.
template <class Items, class Item>
concept ViewOf =
    std::ranges::view<Items> && std::ranges::forward_range<const Items> && std::ranges::sized_range<const Items> &&
    std::convertible_to<std::ranges::range_reference_t<const Items>, const Item&>;

namespace detail {

/// Checks that `version` fits in a header slot of `offset_type`
inline void check_version(std::uint32_t version, OffsetType offset_type) {
    if (version > max_value(offset_type)) {
        throw BuildError::version_too_large(version, offset_type);
    }
}

/// Throws the error of bytes given to a writer that are shorter than its
/// table
[[noreturn]] TAFLA_COLD inline void throw_too_short(std::size_t length, std::size_t given) {
    throw std::invalid_argument("The table is " + std::to_string(length) + " bytes, longer than the " +
                                std::to_string(given) + " bytes given");
}

/// Copies `length` bytes, which may be none from no address, as an empty
/// span's are
TAFLA_ALWAYS_INLINE void copy(std::uint8_t* to, const void* from, std::size_t length) noexcept {
    if (length != 0) {
        std::memcpy(to, from, length);
    }
}

/// Stores the `Size` zero bytes at `to`
template <std::size_t Size>
TAFLA_ALWAYS_INLINE void store_zeros(std::uint8_t* to) noexcept {
    constexpr std::array<std::uint8_t, Size> zeros{};
    std::memcpy(to, zeros.data(), Size);
}

/// Writes `length` zeros. Up to 16, such as a nested table's offsets or an
/// array's padding, are two stores of the same width, which may overlap,
/// rather than a call to `memset()`, across which the caller would keep the
/// writer in registers that the call preserves, or in memory: `TableBuilder`
/// wrote the benchmarks' vector of 100 nested tables in 21% less time so. No
/// padding, the most common, is a single compare, as it was before the call.
TAFLA_ALWAYS_INLINE void zero(std::uint8_t* to, std::size_t length) noexcept {
    if (length == 0) {
        return;
    }

    if (length >= 8) {
        if (length > 16) {
            std::memset(to, 0, length);
            return;
        }
        store_zeros<8>(to);
        store_zeros<8>(to + length - 8);
    } else if (length >= 4) {
        store_zeros<4>(to);
        store_zeros<4>(to + length - 4);
    } else if (length >= 2) {
        store_zeros<2>(to);
        store_zeros<2>(to + length - 2);
    } else if (length == 1) {
        *to = 0;
    }
}

}  // namespace detail

/// Returns the start of `bytes`, which a table of `length` bytes is written
/// into, after checking that they hold it, as `TableSizer::writer(bytes)`
/// does. It's for code that writes a table's bytes itself, at positions that
/// it works out, as tafla-gen's code writes a message of fields of fixed
/// size into bytes that the caller gives.
///
/// Throws `std::invalid_argument` if `bytes` are shorter than `length`.
[[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t* room_for_table(std::span<std::uint8_t> bytes, std::size_t length) {
    if (bytes.size() < length) [[unlikely]] {
        detail::throw_too_short(length, bytes.size());
    }
    return bytes.data();
}

/// The table that a nested table or vector item was started in, which
/// `TableSizer::end_table()` takes back
class [[nodiscard]] SizerParent {
private:
    friend class TableSizer;

    explicit SizerParent(std::uint64_t table) noexcept : table_(table) {}

    std::uint64_t table_;
};

/// Counts the bytes of a table, so that a `TableWriter` can write it into
/// bytes of exactly that length.
///
/// It's for code that knows a table's fields before it writes them, such as
/// the code that tafla-gen generates, which writes tables without recording
/// their fields as `TableBuilder` does.
///
/// The fields are added in order, each with its type's method, which takes
/// what decides its length: nothing for a scalar, the count of an array, the
/// string itself. A nested table is added with `add_table()`, and a vector
/// with `add_vector()`, and the fields of each such table then follow
/// between `start_table()` and `end_table()`. The sizer pads arrays and
/// nested tables as `TableBuilder` does, so a writer that takes the same
/// fields in the same order writes each where the sizer counted it.
///
/// Adding a field doesn't throw: `writer()` throws the `BuildError` of a
/// table too long for its layout's offset type, or of an array of 0-byte
/// structs, as `TableBuilder`'s setter of the field would have.
///
/// ```cpp
/// tafla::TableSizer sizer(2, tafla::TableLayout());
/// sizer.add_float64();
/// sizer.add_string("Europe/Stockholm");
///
/// auto writer = sizer.writer();
/// writer.set_float64(0, 59.3293);
/// writer.set_string(1, "Europe/Stockholm");
/// tafla::Bytes bytes = writer.finish();
/// ```
///
/// A writer from `writer(bytes)` writes the table into bytes that the caller
/// gives it, such as bytes reused for each table, rather than into new bytes:
///
/// ```cpp
/// std::vector<std::uint8_t> buffer(1024);
/// auto writer = sizer.writer(buffer);
/// writer.set_float64(0, 59.3293);
/// writer.set_string(1, "Europe/Stockholm");
/// std::span<const std::uint8_t> table(buffer.data(), writer.finish_into());
/// ```
class TableSizer {
public:
    /// Starts sizing a table of `fields` fields in `layout`, which its
    /// readers must use too. Nested tables use the default layout.
    ///
    /// Throws a `BuildError` if the header alone is too long for the
    /// layout's offset type, as `TableBuilder`'s constructor does.
    TAFLA_ALWAYS_INLINE TableSizer(std::size_t fields, TableLayout<false> layout)
        : fields_(fields),
          sized_(layout.is_sized()),
          offset_type_(layout.offset_type()),
          position_(detail::header_length(fields, layout.is_sized(), false, layout.offset_type())) {}

    /// Starts sizing a table of `fields` fields in `layout`, whose writer
    /// writes `version` in its header.
    ///
    /// Throws a `BuildError` if `version` doesn't fit in the layout's offset
    /// type, or the header alone is too long for it, as `TableBuilder`'s
    /// constructor does.
    TAFLA_ALWAYS_INLINE TableSizer(std::size_t fields, TableLayout<true> layout, std::uint32_t version)
        : fields_(fields),
          version_(version),
          versioned_(true),
          sized_(layout.is_sized()),
          offset_type_(layout.offset_type()),
          position_(checked_header(fields, layout, version)) {}

    /// The byte length of the table, with every field added.
    ///
    /// Throws a `BuildError` if the table is too long for its layout's
    /// offset type: longer than 255 bytes, 64 KiB or 4 GiB. It names the
    /// root field that made it too long, and the length that the root's
    /// fields up to it make the table, as `TableBuilder`'s setter of the
    /// field throws. It also throws if an array of 0-byte structs was added,
    /// unless a field before it made the table too long.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t byte_length() const {
        if (error_) [[unlikely]] {
            detail::throw_build_error(*error_);
        }

        if (position_ > max_value(offset_type_) || position_ > SIZE_MAX) [[unlikely]] {
            const auto index = root_fields_ == 0 ? 0 : root_fields_ - 1;
            detail::throw_build_error({BuildError::Kind::too_long, offset_type_, index, position_});
        }
        return static_cast<std::size_t>(position_);
    }

    /// Returns a writer of the table, into new bytes of its length, which
    /// takes its fields in the order that they were added.
    ///
    /// Throws the `BuildError` of `byte_length()`, before allocating the
    /// bytes.
    [[nodiscard]] TableWriter writer() const;

    /// Returns a writer of the table into the start of `bytes`, rather than
    /// into new bytes, which takes its fields in the order that they were
    /// added. Its `finish_into()` returns the table's length, the number of
    /// bytes that hold it. It writes every byte of the table, padding
    /// included, so the bytes needn't be zero, and leaves those after it as
    /// they were.
    ///
    /// The caller owns the bytes, which must outlive the writer. The table
    /// stays in them until they're written again, such as by the next table
    /// written into them, which also changes what a reader of them reads.
    /// `ArrayView::as_span()` views the table's arrays in place only when the
    /// table starts at an address aligned to their elements, as the start of
    /// `Bytes`, a `std::vector` or other memory from `new` is to 8.
    ///
    /// Reusing bytes saves allocating new ones for each table: the generated
    /// code wrote a record of 60 bytes in a quarter of the time, and a table
    /// of 1.3 KB in two thirds, as cpp/README.md describes.
    ///
    /// Throws the `BuildError` of `byte_length()`, or `std::invalid_argument`
    /// if `bytes` are shorter than the table, before writing any of them.
    [[nodiscard]] TableWriter writer(std::span<std::uint8_t> bytes) const;

    /// Starts the fields of a nested table or vector item of `fields` fields,
    /// after its parent's `add_table()` or `add_vector()`
    [[nodiscard]] TAFLA_ALWAYS_INLINE SizerParent start_table(std::size_t fields) noexcept {
        SizerParent parent(table_);
        table_ = position_;
        add_saturating(detail::saturating_product(fields, width()));
        return parent;
    }

    /// Ends the fields of the table that `start_table()` started
    TAFLA_ALWAYS_INLINE void end_table(SizerParent parent) noexcept {
        table_ = parent.table_;
    }

    /// Adds a `std::int8_t` field. So do the other scalars' adders.
    TAFLA_ALWAYS_INLINE void add_int8() noexcept {
        add(1);
    }

    TAFLA_ALWAYS_INLINE void add_uint8() noexcept {
        add(1);
    }

    TAFLA_ALWAYS_INLINE void add_int16() noexcept {
        add(2);
    }

    TAFLA_ALWAYS_INLINE void add_uint16() noexcept {
        add(2);
    }

    TAFLA_ALWAYS_INLINE void add_int32() noexcept {
        add(4);
    }

    TAFLA_ALWAYS_INLINE void add_uint32() noexcept {
        add(4);
    }

    TAFLA_ALWAYS_INLINE void add_int64() noexcept {
        add(8);
    }

    TAFLA_ALWAYS_INLINE void add_uint64() noexcept {
        add(8);
    }

    TAFLA_ALWAYS_INLINE void add_float32() noexcept {
        add(4);
    }

    TAFLA_ALWAYS_INLINE void add_float64() noexcept {
        add(8);
    }

    /// Adds an array field of `count` `std::int8_t` elements. So do the other
    /// arrays' adders.
    TAFLA_ALWAYS_INLINE void add_int8_array(std::size_t count) noexcept {
        add_elements<std::int8_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_uint8_array(std::size_t count) noexcept {
        add_elements<std::uint8_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_int16_array(std::size_t count) noexcept {
        add_elements<std::int16_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_uint16_array(std::size_t count) noexcept {
        add_elements<std::uint16_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_int32_array(std::size_t count) noexcept {
        add_elements<std::int32_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_uint32_array(std::size_t count) noexcept {
        add_elements<std::uint32_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_int64_array(std::size_t count) noexcept {
        add_elements<std::int64_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_uint64_array(std::size_t count) noexcept {
        add_elements<std::uint64_t>(count);
    }

    TAFLA_ALWAYS_INLINE void add_float32_array(std::size_t count) noexcept {
        add_elements<float>(count);
    }

    TAFLA_ALWAYS_INLINE void add_float64_array(std::size_t count) noexcept {
        add_elements<double>(count);
    }

    /// Adds a string field, of its UTF-8 bytes
    TAFLA_ALWAYS_INLINE void add_string(std::string_view string) noexcept {
        add(detail::saturating_sum(width(), string.size()));
    }

    /// Adds a struct field of `size` bytes
    TAFLA_ALWAYS_INLINE void add_struct(std::size_t size) noexcept {
        add(size);
    }

    /// Adds an array of `count` structs of `size` bytes each, padded so that
    /// they start at a multiple of `alignment` from the start of the table.
    ///
    /// `writer()` throws a `BuildError` if `size` is 0, as
    /// `TableBuilder::set_struct_array()` does.
    TAFLA_ALWAYS_INLINE void add_struct_array(std::size_t count, std::size_t size, Alignment alignment) noexcept {
        if (size == 0 && !error_) {
            error_ = BuildError::Details{BuildError::Kind::not_whole_structs, OffsetType::uint32, 0, 0};
        }
        add_array(detail::saturating_product(count, size), alignment);
    }

    /// Adds a nested table, padded to start at a multiple of `alignment`,
    /// whose fields then follow between `start_table()` and `end_table()`
    TAFLA_ALWAYS_INLINE void add_table(Alignment alignment) noexcept {
        start_field();
        add_saturating(detail::padding(position_ - table_, alignment));
    }

    /// Adds a vector of `count` tables, whose fields then follow, each
    /// between `start_table()` and `end_table()`
    TAFLA_ALWAYS_INLINE void add_vector(std::size_t count) noexcept {
        add(detail::vector_bytes(count, width()));
    }

private:
    TAFLA_ALWAYS_INLINE static std::uint64_t checked_header(std::size_t fields, TableLayout<true> layout,
                                                            std::uint32_t version) {
        detail::check_version(version, layout.offset_type());
        return detail::header_length(fields, layout.is_sized(), true, layout.offset_type());
    }

    template <class T>
    TAFLA_ALWAYS_INLINE void add_elements(std::size_t count) noexcept {
        add_array(detail::saturating_product(count, sizeof(T)), static_cast<Alignment>(sizeof(T)));
    }

    /// Adds a field of `byte_length` bytes, without padding
    TAFLA_ALWAYS_INLINE void add(std::uint64_t byte_length) noexcept {
        start_field();
        add_saturating(byte_length);
    }

    /// Adds an array of `byte_length` bytes after its count, padded so that
    /// its elements are aligned
    TAFLA_ALWAYS_INLINE void add_array(std::uint64_t byte_length, Alignment alignment) noexcept {
        start_field();
        add_saturating(detail::array_bytes(position_ - table_, width(), byte_length, alignment));
    }

    /// Counts a field of the root, after checking the length that the root's
    /// fields before it make the table. The length is checked only between
    /// the root's fields, so that it's the length after the whole field that
    /// made the table too long, as `TableBuilder` reports it.
    TAFLA_ALWAYS_INLINE void start_field() noexcept {
        // Only the root starts at 0, since a nested table follows the offset
        // of the field that holds it
        if (table_ != 0) {
            return;
        }

        if (!error_ && position_ > max_value(offset_type_)) {
            const auto index = root_fields_ == 0 ? 0 : root_fields_ - 1;
            error_ = BuildError::Details{BuildError::Kind::too_long, offset_type_, index, position_};
        }
        ++root_fields_;
    }

    /// Adds `byte_length` bytes to the table's length, which saturates as
    /// `detail::saturating_sum()` describes
    TAFLA_ALWAYS_INLINE void add_saturating(std::uint64_t byte_length) noexcept {
        position_ = detail::saturating_sum(position_, byte_length);
    }

    [[nodiscard]] std::uint64_t width() const noexcept {
        return offset_size(offset_type_);
    }

    /// The number of the root's fields
    std::size_t fields_;
    std::uint32_t version_ = 0;
    bool versioned_ = false;
    bool sized_;
    OffsetType offset_type_;
    /// The table's length so far, which is where the next field goes
    std::uint64_t position_;
    /// The start of the table being sized, which arrays are aligned from: 0
    /// for the root, and past its header for the tables nested in it, so
    /// that `start_field()` tells the root's fields by it
    std::uint64_t table_ = 0;
    /// The root's fields added so far, the last of which a table too long
    /// for its offsets names
    std::uint64_t root_fields_ = 0;
    /// The first error: of an array of 0-byte structs, or of the first root
    /// field that made the table too long
    std::optional<BuildError::Details> error_;
};

/// The table that a nested table or vector item was started in, which
/// `TableWriter::end_table()` takes back
class [[nodiscard]] WriterParent {
private:
    template <bool>
    friend class BasicTableWriter;
    /// Keeps them in the stack of its walk of the tables nested in place
    friend class TableBuilder;

    WriterParent() noexcept = default;

    WriterParent(std::size_t table, std::size_t fields) noexcept : table_(table), fields_(fields) {}

    std::size_t table_;
    std::size_t fields_;
};

/// The position of a vector that `TableWriter::set_vector()` wrote, and its
/// count, which `TableWriter::set_item_end()` takes
class VectorPosition {
private:
    template <bool>
    friend class BasicTableWriter;
    friend class TableBuilder;

    VectorPosition() noexcept = default;

    VectorPosition(std::size_t position, std::size_t count) noexcept : position_(position), count_(count) {}

    std::size_t position_;
    std::size_t count_;
};

/// Writes a table forward into bytes of exactly its length, which a
/// `TableSizer` counted: new bytes, which `finish()` returns, or the start of
/// bytes that the caller gave, whose table `finish_into()` checks.
///
/// It doesn't record the fields first, as `TableBuilder` does before it
/// writes them through a writer of its own. It comes from
/// `TableSizer::writer()`, and takes the fields that the sizer added, in the
/// same order, each with its index in its table.
///
/// Each field's offset is written as the writer reaches the field. A nested
/// table follows its parent's `set_table()`, between `start_table()` and
/// `end_table()`; the items of a vector follow its `set_vector()` the same
/// way, each followed by `set_item_end()`, which writes where it ends. Arrays
/// and nested tables are padded so that their elements are aligned from
/// their table's start, as the sizer counts them.
///
/// It's the target of the code that tafla-gen generates, whose calls follow
/// from the schema: fields are set in index order, each once, as the sizer
/// added them, and each vector item is followed by its `set_item_end()`.
/// Hand-written tables use `TableBuilder`, which checks every field.
///
/// Writing a field doesn't throw: `finish()` and `finish_into()` throw the
/// `BuildError` of fields that differ from those that the sizer added. The
/// writer never writes past the table's length, nor allocates more than it,
/// whatever it's given. Like Rust's, it also reports an index past its
/// table's fields, or an item past its vector's count; the contract that the
/// runtimes share leaves the table's bytes undefined for them.
///
/// `TableWriter` is `BasicTableWriter<true>`. `TableBuilder` writes through
/// `BasicTableWriter<false>`, which no other code constructs, and which
/// skips those checks of room, indexes and items, since the builder's
/// setters checked each field as they recorded it. Its `finish()` still
/// checks the length that it wrote.
template <bool Checked>
class BasicTableWriter {
public:
    /// Moves `other`'s bytes and position, and leaves it without bytes, so
    /// that it writes nothing and its `finish()` or `finish_into()` throws,
    /// unless its table is empty
    BasicTableWriter(BasicTableWriter&& other) noexcept
        : bytes_(std::move(other.bytes_)),
          begin_(other.begin_),
          cursor_(other.cursor_),
          end_(other.end_),
          length_(other.length_),
          offset_type_(other.offset_type_),
          given_(other.given_),
          header_size_(other.header_size_),
          table_(other.table_),
          slots_(other.slots_),
          fields_(other.fields_),
          error_(other.error_) {
        other.forget_bytes();
    }

    BasicTableWriter& operator=(BasicTableWriter&& other) noexcept {
        if (this != &other) {
            bytes_ = std::move(other.bytes_);
            begin_ = other.begin_;
            cursor_ = other.cursor_;
            end_ = other.end_;
            length_ = other.length_;
            offset_type_ = other.offset_type_;
            given_ = other.given_;
            header_size_ = other.header_size_;
            table_ = other.table_;
            slots_ = other.slots_;
            fields_ = other.fields_;
            error_ = other.error_;
            other.forget_bytes();
        }
        return *this;
    }

    BasicTableWriter(const BasicTableWriter&) = delete;
    BasicTableWriter& operator=(const BasicTableWriter&) = delete;
    ~BasicTableWriter() = default;

    /// Returns the table's bytes, and leaves the writer without them.
    ///
    /// Throws a `BuildError` if the fields didn't fill the bytes, as when
    /// they differ from those that the sizer added, if one was written at an
    /// index past its table's fields, or if an array of structs was given
    /// bytes that aren't whole structs. The bytes, of which the writer
    /// wrote only some, are then freed with the writer. Throws
    /// `std::logic_error` if the writer writes into bytes that the caller
    /// gave, whose table `finish_into()` checks instead.
    [[nodiscard]] TAFLA_ALWAYS_INLINE Bytes finish() {
        if (given_) [[unlikely]] {
            throw_finished_as("tafla::TableWriter::finish: the writer writes into given bytes; use finish_into()");
        }
        check();

        forget_bytes();
        return std::move(bytes_);
    }

    /// Checks the table that a writer from `TableSizer::writer(bytes)` wrote
    /// into the start of those bytes, returns its length, the number of bytes
    /// that hold it, and leaves the writer without the bytes.
    ///
    /// Throws the `BuildError` that `finish()` throws, and leaves the bytes
    /// with what the writer wrote of the table, or `std::logic_error` if the
    /// writer writes into new bytes, which `finish()` returns.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t finish_into() {
        if (!given_) [[unlikely]] {
            throw_finished_as("tafla::TableWriter::finish_into: the writer writes into new bytes; use finish()");
        }
        check();

        forget_bytes();
        return length_;
    }

    /// Starts the fields of a nested table or vector item of `fields`
    /// fields, after its parent's `set_table()` or `set_vector()`
    [[nodiscard]] TAFLA_ALWAYS_INLINE WriterParent start_table(std::size_t fields) noexcept {
        WriterParent parent(table_, fields_);

        const std::size_t start = written();
        table_ = start;
        slots_ = start;

        // Without room for its slots, the table has no fields to write, so
        // that no slot is looked for past the bytes
        const std::size_t width = offset_size(offset_type_);
        if (!Checked || fields <= room() / width) {
            push_zeros(fields * width);
            fields_ = fields;
        } else {
            fail();
            fields_ = 0;
        }

        return parent;
    }

    /// Ends the fields of the table that `start_table()` started
    TAFLA_ALWAYS_INLINE void end_table(WriterParent parent) noexcept {
        table_ = parent.table_;
        fields_ = parent.fields_;
        // Only the root starts at 0, and only its offsets follow a header
        slots_ = parent.table_ == 0 ? header_size_ : parent.table_;
    }

    /// Writes `std::int8_t` field `index`. So do the other scalars' setters.
    TAFLA_ALWAYS_INLINE void set_int8(std::size_t index, std::int8_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_uint8(std::size_t index, std::uint8_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_int16(std::size_t index, std::int16_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_uint16(std::size_t index, std::uint16_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_int32(std::size_t index, std::int32_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_uint32(std::size_t index, std::uint32_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_int64(std::size_t index, std::int64_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_uint64(std::size_t index, std::uint64_t value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_float32(std::size_t index, float value) noexcept {
        set_scalar(index, value);
    }

    TAFLA_ALWAYS_INLINE void set_float64(std::size_t index, double value) noexcept {
        set_scalar(index, value);
    }

    /// Writes array field `index` of `std::int8_t` elements. So do the other
    /// arrays' setters.
    TAFLA_ALWAYS_INLINE void set_int8_array(std::size_t index, std::span<const std::int8_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_uint8_array(std::size_t index, std::span<const std::uint8_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_int16_array(std::size_t index, std::span<const std::int16_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_uint16_array(std::size_t index, std::span<const std::uint16_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_int32_array(std::size_t index, std::span<const std::int32_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_uint32_array(std::size_t index, std::span<const std::uint32_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_int64_array(std::size_t index, std::span<const std::int64_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_uint64_array(std::size_t index, std::span<const std::uint64_t> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_float32_array(std::size_t index, std::span<const float> values) noexcept {
        set_elements(index, values);
    }

    TAFLA_ALWAYS_INLINE void set_float64_array(std::size_t index, std::span<const double> values) noexcept {
        set_elements(index, values);
    }

    /// Writes string field `index`, as its UTF-8 bytes
    TAFLA_ALWAYS_INLINE void set_string(std::size_t index, std::string_view string) noexcept {
        set_offset(index);
        push_slot(string.size());
        push(string.data(), string.size());
    }

    /// Writes struct field `index` from its bytes: its scalars,
    /// little-endian, one after another, which are written as they are,
    /// without padding
    TAFLA_ALWAYS_INLINE void set_struct(std::size_t index, std::span<const std::uint8_t> bytes) noexcept {
        set_offset(index);
        push(bytes.data(), bytes.size());
    }

    /// Writes field `index`, an array of structs of `size` bytes each, from
    /// their bytes one after another, which it counts. It's padded so that
    /// the structs start at a multiple of `alignment` from the start of the
    /// table, which is the size of the structs' largest scalar.
    ///
    /// `finish()` throws a `BuildError` if `size` is 0 or the bytes aren't a
    /// whole number of structs.
    TAFLA_ALWAYS_INLINE void set_struct_array(std::size_t index, std::span<const std::uint8_t> bytes, std::size_t size,
                                              Alignment alignment) noexcept {
        const std::size_t length = bytes.size();
        if (size == 0 || length % size != 0) [[unlikely]] {
            record({BuildError::Kind::not_whole_structs, OffsetType::uint32, length, size});
            return;
        }

        start_array(index, length / size, alignment);
        push(bytes.data(), length);
    }

    /// Writes struct field `index`, as the bytes that `value.to_bytes()`
    /// writes, without padding. It throws only what `to_bytes()` throws,
    /// which tafla-gen's structs don't.
    template <Struct T>
    TAFLA_ALWAYS_INLINE void set_struct(std::size_t index, const T& value) noexcept(
        noexcept(value.to_bytes(std::declval<std::uint8_t*>()))) {
        set_offset(index);
        if (!Checked || T::SIZE <= room()) [[likely]] {
            value.to_bytes(cursor_);
            cursor_ += T::SIZE;
        } else {
            fail();
        }
    }

    /// Writes field `index`, an array of `structs`, each as the bytes that its
    /// `to_bytes()` writes, padded so that they start at a multiple of
    /// `T::ALIGNMENT` from the start of the table. It throws only what
    /// `to_bytes()` throws.
    template <Struct T>
    TAFLA_ALWAYS_INLINE void set_structs(std::size_t index, std::span<const T> structs) noexcept(
        noexcept(std::declval<const T&>().to_bytes(std::declval<std::uint8_t*>()))) {
        const auto write = [](const T& item, std::uint8_t* bytes) noexcept(noexcept(item.to_bytes(bytes))) {
            item.to_bytes(bytes);
        };
        set_structs<T::SIZE>(index, structs, write, T::ALIGNMENT);
    }

    /// Writes field `index`, an array of a `T` for each of `structs`, which
    /// are `T`s or convert into them, each as the bytes that `T::to_bytes()`
    /// writes, as the overload of a span of `T`s does. tafla-gen's generic
    /// fields structs write their views of the caller's items with it. It
    /// throws only what reading an item from `structs`, converting it or
    /// `to_bytes()` throws.
    template <Struct T, std::ranges::sized_range Structs>
        requires std::convertible_to<std::ranges::range_reference_t<const Structs>, const T&>
    TAFLA_ALWAYS_INLINE void set_structs(std::size_t index, const Structs& structs) noexcept(
        noexcept(std::declval<const T&>().to_bytes(std::declval<std::uint8_t*>())) &&
        noexcept(static_cast<const T&>(*std::ranges::begin(structs)))) {
        const auto write = [](const T& item, std::uint8_t* bytes) noexcept(noexcept(item.to_bytes(bytes))) {
            item.to_bytes(bytes);
        };
        set_structs<T::SIZE>(index, structs, write, T::ALIGNMENT);
    }

    /// Writes field `index`, an array of a struct for each of `structs`, each
    /// the `Size` bytes that `write(item, bytes)` writes at `bytes` for it:
    /// its scalars, little-endian, one after another. It writes them as
    /// `set_struct_array()` writes the bytes of every struct, without
    /// collecting them first. It's padded so that the structs start at a
    /// multiple of `alignment` from the start of the table, which is the size
    /// of the structs' largest scalar.
    ///
    /// `finish()` throws a `BuildError` if `Size` is 0. It throws only what
    /// `write` throws.
    template <std::size_t Size, std::ranges::sized_range Structs, class Write>
    TAFLA_ALWAYS_INLINE void set_structs(std::size_t index, const Structs& structs, Write write,
                                         Alignment alignment) noexcept(noexcept(write(*std::ranges::begin(structs),
                                                                                      std::declval<std::uint8_t*>()))) {
        if constexpr (Size == 0) {
            record({BuildError::Kind::not_whole_structs, OffsetType::uint32, 0, 0});
            return;
        } else {
            const auto count = static_cast<std::size_t>(std::ranges::size(structs));
            start_array(index, count, alignment);

            if (Checked && count > room() / Size) [[unlikely]] {
                fail();
                return;
            }

            for (const auto& item : structs) {
                write(item, cursor_);
                cursor_ += Size;
            }
        }
    }

    /// Writes the offset of nested table field `index`, padded to start at a
    /// multiple of `alignment`, whose fields then follow between
    /// `start_table()` and `end_table()`
    TAFLA_ALWAYS_INLINE void set_table(std::size_t index, Alignment alignment) noexcept {
        push_zeros(detail::padding(written() - table_, alignment));
        set_offset(index);
    }

    /// Writes the offset and count of vector field `index`, of `count`
    /// tables, whose items then follow, each between `start_table()` and
    /// `end_table()` and then `set_item_end()`
    [[nodiscard]] TAFLA_ALWAYS_INLINE VectorPosition set_vector(std::size_t index, std::size_t count) noexcept {
        set_offset(index);

        // The count, and then the end of each item, which is written after
        // the item. Without room for them, the vector has no items to end.
        const std::size_t position = written();
        const std::size_t width = offset_size(offset_type_);
        if (Checked && count >= room() / width) [[unlikely]] {
            fail();
            return {position, 0};
        }

        push_slot(count);
        push_zeros(count * width);

        return {position, count};
    }

    /// Writes the end of item `item` of the vector, which was just written
    TAFLA_ALWAYS_INLINE void set_item_end(VectorPosition vector, std::size_t item) noexcept {
        if (Checked && item >= vector.count_) [[unlikely]] {
            fail();
            return;
        }

        // Each end counts from the vector's position, which is its count's
        const std::size_t slot = vector.position_ + (item + 1) * offset_size(offset_type_);
        const std::size_t end = written() > vector.position_ ? written() - vector.position_ : 0;
        set_slot(slot, end);
    }

    /// Writes a finished table's bytes as they are, in place of the fields
    /// that `start_table()` and `end_table()` would enclose: after its
    /// parent's `set_table()`, or as a vector item before its
    /// `set_item_end()`. It's for tables nested as bytes, and builders with
    /// a header or another offset type, which can't be written in place.
    TAFLA_ALWAYS_INLINE void copy_table(std::span<const std::uint8_t> table) noexcept {
        push(table.data(), table.size());
    }

private:
    friend class TableSizer;
    friend class TableBuilder;

    /// Writes the header of the root table of `fields` fields, into new bytes
    /// of `length`, which the sizer or `TableBuilder` checked are at least the
    /// header and at most what the offset type addresses
    TAFLA_ALWAYS_INLINE BasicTableWriter(std::size_t length, std::size_t fields, bool sized,
                                         std::optional<std::uint32_t> version, OffsetType offset_type)
        : bytes_(Bytes::for_overwrite(length)),
          length_(length),
          offset_type_(offset_type),
          header_size_(((sized ? 1U : 0U) + (version ? 1U : 0U)) * offset_size(offset_type)),
          slots_(header_size_),
          fields_(fields) {
        start(bytes_.data(), fields, sized, version);
    }

    /// Writes the header as the constructor above does, into the start of
    /// `bytes`, which the sizer checked are at least `length`
    TAFLA_ALWAYS_INLINE BasicTableWriter(std::span<std::uint8_t> bytes, std::size_t length, std::size_t fields,
                                         bool sized, std::optional<std::uint32_t> version, OffsetType offset_type)
        : length_(length),
          offset_type_(offset_type),
          given_(true),
          header_size_(((sized ? 1U : 0U) + (version ? 1U : 0U)) * offset_size(offset_type)),
          slots_(header_size_),
          fields_(fields) {
        start(bytes.data(), fields, sized, version);
    }

    /// Writes the header of the table that `bytes` start, which are its
    /// length long
    TAFLA_ALWAYS_INLINE void start(std::uint8_t* bytes, std::size_t fields, bool sized,
                                   std::optional<std::uint32_t> version) noexcept {
        begin_ = length_ == 0 ? empty_ : bytes;
        cursor_ = begin_;
        end_ = begin_ + length_;

        if (sized) {
            push_slot(length_);
        }

        if (version) {
            push_slot(*version);
        }

        // Each field's offset is written as the writer reaches the field
        if (!Checked || fields <= room() / offset_size(offset_type_)) {
            push_zeros(fields * offset_size(offset_type_));
        } else {
            fail();
        }
    }

    /// Throws the `BuildError` of fields that didn't fill the table, as
    /// `finish()` and `finish_into()` do
    TAFLA_ALWAYS_INLINE void check() const {
        if (error_) [[unlikely]] {
            detail::throw_build_error(*error_);
        }

        if (written() != length_) [[unlikely]] {
            detail::throw_build_error({BuildError::Kind::not_as_sized, offset_type_, written(), length_});
        }
    }

    /// Throws the error of finishing a writer as the other kind: of new
    /// bytes, or of bytes that the caller gave
    [[noreturn]] TAFLA_COLD static void throw_finished_as(const char* message) {
        throw std::logic_error(message);
    }

    template <class T>
    TAFLA_ALWAYS_INLINE void set_scalar(std::size_t index, T value) noexcept {
        set_offset(index);
        push_scalar(value);
    }

    /// Appends a scalar, little-endian, if the table has room for it
    template <class T>
    TAFLA_ALWAYS_INLINE void push_scalar(T value) noexcept {
        if (!Checked || sizeof(T) <= room()) [[likely]] {
            detail::store(cursor_, value);
            cursor_ += sizeof(T);
        } else {
            fail();
        }
    }

    /// Appends `count` elements, little-endian, one by one, if the table has
    /// room for them
    template <class T>
    TAFLA_ALWAYS_INLINE void push_elements(const T* values, std::size_t count) noexcept {
        if (Checked && count > room() / sizeof(T)) [[unlikely]] {
            fail();
            return;
        }

        for (std::size_t index = 0; index < count; ++index) {
            T value;
            std::memcpy(&value, values + index, sizeof value);
            detail::store(cursor_, value);
            cursor_ += sizeof(T);
        }
    }

    template <class T>
    TAFLA_ALWAYS_INLINE void set_elements(std::size_t index, std::span<const T> values) noexcept {
        start_array(index, values.size(), static_cast<Alignment>(sizeof(T)));

        if (Checked && values.size() > room() / sizeof(T)) [[unlikely]] {
            fail();
            return;
        }

        if constexpr (detail::kHostOrder || sizeof(T) == 1) {
            detail::copy(cursor_, values.data(), values.size_bytes());
            cursor_ += values.size_bytes();
        } else {
            for (const T value : values) {
                detail::store(cursor_, value);
                cursor_ += sizeof(T);
            }
        }
    }

    /// Pads an array so that its elements, after its count, are aligned, and
    /// writes its offset and count
    TAFLA_ALWAYS_INLINE void start_array(std::size_t index, std::size_t count, Alignment alignment) noexcept {
        // Wrapping, since only the remainder matters
        const std::size_t offset = written() + offset_size(offset_type_) - table_;
        push_zeros(detail::padding(offset, alignment));
        set_offset(index);
        push_slot(count);
    }

    /// Writes the offset of field `index` of the table being written, which
    /// is where the field starts
    TAFLA_ALWAYS_INLINE void set_offset(std::size_t index) noexcept {
        if (Checked && index >= fields_) [[unlikely]] {
            record({BuildError::Kind::too_many_fields, OffsetType::uint32, fields_, 0});
            return;
        }

        // The table's slots were written when it started, so the slot lies
        // within the bytes, and the offset within the table's length
        const std::size_t slot = slots_ + index * offset_size(offset_type_);
        const std::size_t offset = written() > table_ ? written() - table_ : 0;
        set_slot(slot, offset);
    }

    /// The bytes written so far
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t written() const noexcept {
        return static_cast<std::size_t>(cursor_ - begin_);
    }

    /// The bytes left of the table's length
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t room() const noexcept {
        return static_cast<std::size_t>(end_ - cursor_);
    }

    /// Appends `length` bytes if the table has room for them
    TAFLA_ALWAYS_INLINE void push(const void* bytes, std::size_t length) noexcept {
        if (!Checked || length <= room()) [[likely]] {
            detail::copy(cursor_, bytes, length);
            cursor_ += length;
        } else {
            fail();
        }
    }

    /// Appends `count` zeros, of padding or of slots written later, if the
    /// table has room for them
    TAFLA_ALWAYS_INLINE void push_zeros(std::size_t count) noexcept {
        if (!Checked || count <= room()) [[likely]] {
            detail::zero(cursor_, count);
            cursor_ += count;
        } else {
            fail();
        }
    }

    // The slots, like the scalars, are stored at constant lengths, which are
    // single stores rather than calls to copy bytes. A value is at most the
    // table's length, which fits in the offsets, or it's truncated after a
    // write past the length failed.

    /// Appends a count, length or header slot
    TAFLA_ALWAYS_INLINE void push_slot(std::size_t value) noexcept {
        const std::size_t width = offset_size(offset_type_);
        if (!Checked || width <= room()) [[likely]] {
            store_slot(cursor_, value);
            cursor_ += width;
        } else {
            fail();
        }
    }

    /// Sets the offset or item end at `position`, if the bytes hold it
    TAFLA_ALWAYS_INLINE void set_slot(std::size_t position, std::size_t value) noexcept {
        if (!Checked || (position <= written() && offset_size(offset_type_) <= written() - position)) [[likely]] {
            store_slot(begin_ + position, value);
        } else {
            fail();
        }
    }

    TAFLA_ALWAYS_INLINE void store_slot(std::uint8_t* at, std::size_t value) const noexcept {
        switch (offset_type_) {
            case OffsetType::uint8:
                *at = static_cast<std::uint8_t>(value);
                return;
            case OffsetType::uint16:
                detail::store(at, static_cast<std::uint16_t>(value));
                return;
            case OffsetType::uint32:
                break;
        }
        detail::store(at, static_cast<std::uint32_t>(value));
    }

    /// Leaves the writer without bytes to write, as `finish()` and a move
    /// leave it, so that a write past them fails rather than writing into
    /// bytes that it no longer holds
    TAFLA_ALWAYS_INLINE void forget_bytes() noexcept {
        cursor_ = end_ = begin_ = empty_;
    }

    /// Records the first error, which `finish()` throws
    TAFLA_ALWAYS_INLINE void record(BuildError::Details details) noexcept {
        if (!error_) {
            error_ = details;
        }
    }

    /// Records that the fields differ from those sized, where the first that
    /// did was written. Inline, as is every path of the writer, rather than a
    /// call that takes the writer's address, which would keep its fields in
    /// memory rather than in registers.
    TAFLA_ALWAYS_INLINE void fail() noexcept {
        record({BuildError::Kind::not_as_sized, offset_type_, written(), length_});
    }

    /// Where the bytes of an empty table point, so that copying no bytes
    /// there has an address. A table of length 0 has no header, but GCC's
    /// `-Wstringop-overflow` sees an unchecked writer write the header's two
    /// slots here, so it has room for them. Returning before them instead
    /// made `TableBuilder::to_bytes()` 4% slower, through Clang's allocation
    /// of its registers.
    static inline std::uint8_t empty_[8] = {};

    Bytes bytes_;
    std::uint8_t* begin_ = nullptr;
    std::uint8_t* cursor_ = nullptr;
    std::uint8_t* end_ = nullptr;
    /// The table's length, which the sizer or `TableBuilder` counted
    std::size_t length_;
    OffsetType offset_type_;
    /// Whether the writer writes into bytes that the caller gave, rather
    /// than into `bytes_`
    bool given_ = false;
    /// The byte length of the root's header slots, which its offsets follow
    std::size_t header_size_;
    /// The start of the table being written, which its offsets count from: 0
    /// for the root, which `end_table()` tells from the tables nested in it
    /// by that, since a nested table starts after its parent's offsets
    std::size_t table_ = 0;
    /// The position of that table's first offset
    std::size_t slots_;
    /// The number of that table's fields
    std::size_t fields_;
    /// The first error, which `finish()` throws
    std::optional<BuildError::Details> error_;
};

TAFLA_ALWAYS_INLINE TableWriter TableSizer::writer() const {
    const std::size_t length = byte_length();
    std::optional<std::uint32_t> version;
    if (versioned_) {
        version = version_;
    }

    return {length, fields_, sized_, version, offset_type_};
}

TAFLA_ALWAYS_INLINE TableWriter TableSizer::writer(std::span<std::uint8_t> bytes) const {
    const std::size_t length = byte_length();
    auto* const start = room_for_table(bytes, length);

    std::optional<std::uint32_t> version;
    if (versioned_) {
        version = version_;
    }

    return {{start, bytes.size()}, length, fields_, sized_, version, offset_type_};
}

}  // namespace tafla

#endif
