// A prototype of a table builder that writes each field's bytes when it's
// set, rather than recording the field and writing every one in
// `to_bytes()`, as `tafla::TableBuilder` does. See README.md beside it.

#ifndef TAFLA_PROTOTYPE_DIRECT_TABLE_BUILDER_HPP
#define TAFLA_PROTOTYPE_DIRECT_TABLE_BUILDER_HPP

#include <tafla/tafla.hpp>

#include <algorithm>
#include <array>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iterator>
#include <memory>
#include <new>
#include <optional>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

namespace tafla::prototype {

/// Borrowed or owned runs of at least this many bytes, an array's elements, a
/// string's or a nested table's bytes, are kept as segments of the table,
/// which `to_bytes()` copies, rather than copied into the builder's bytes
/// when they're set
inline constexpr std::size_t kSegmentBytes = 256;

/// A threshold that keeps no segment: every value is copied when it's set
inline constexpr std::size_t kNoSegments = SIZE_MAX;

template <std::size_t SegmentBytes>
class BasicDirectTableBuilder;

template <std::size_t SegmentBytes>
class BasicDirectNestedTable;

/// The builder that keeps runs of `kSegmentBytes` or more as segments
using DirectTableBuilder = BasicDirectTableBuilder<kSegmentBytes>;

/// The builder that copies every value into its bytes when it's set
using CopyingTableBuilder = BasicDirectTableBuilder<kNoSegments>;

namespace detail {

/// The bytes that a builder has written: the first `Inline` within it, and
/// more on the heap, which grows at least twofold, keeping what's written
class Buffer {
public:
    static constexpr std::size_t Inline = 128;
    /// The least that the bytes grow to on the heap, so that a table that
    /// leaves the builder's own bytes doesn't grow again soon
    static constexpr std::size_t FirstHeap = 512;

    Buffer() noexcept = default;

    Buffer(Buffer&& other) noexcept {
        take(other);
    }

    Buffer& operator=(Buffer&& other) noexcept {
        if (this != &other) {
            release();
            take(other);
        }
        return *this;
    }

    Buffer(const Buffer&) = delete;
    Buffer& operator=(const Buffer&) = delete;

    ~Buffer() {
        release();
    }

    [[nodiscard]] std::uint8_t* data() noexcept {
        return data_;
    }

    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return data_;
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    /// Adds `count` bytes, which the caller writes, and returns where they
    /// start. Leaves the bytes as they were if growing them fails.
    TAFLA_ALWAYS_INLINE std::uint8_t* extend(std::size_t count) {
        if (count > capacity_ - size_) [[unlikely]] {
            grow(count);
        }
        std::uint8_t* at = data_ + size_;
        size_ += count;
        return at;
    }

    /// Makes room for `count` more bytes, so that adding them doesn't grow
    /// the bytes
    void reserve_more(std::size_t count) {
        if (count > capacity_ - size_) {
            grow(count);
        }
    }

    /// Removes the bytes past the first `size`
    void truncate(std::size_t size) noexcept {
        size_ = size;
    }

private:
    [[gnu::noinline]] void grow(std::size_t count) {
        if (count > SIZE_MAX / 2 - size_) {
            throw std::bad_alloc();
        }

        const std::size_t capacity = (std::max)({size_ + count, 2 * capacity_, FirstHeap});
        auto* grown = static_cast<std::uint8_t*>(::operator new(capacity));
        if (size_ != 0) {
            std::memcpy(grown, data_, size_);
        }
        release();
        data_ = grown;
        capacity_ = capacity;
    }

    [[nodiscard]] bool is_inline() const noexcept {
        return data_ == inline_;
    }

    void release() noexcept {
        if (!is_inline()) {
            ::operator delete(data_);
        }
    }

    /// Takes `other`'s bytes, and leaves it with none, within itself
    void take(Buffer& other) noexcept {
        size_ = other.size_;
        if (other.is_inline()) {
            std::memcpy(inline_, other.inline_, other.size_);
            data_ = inline_;
            capacity_ = Inline;
        } else {
            data_ = other.data_;
            capacity_ = other.capacity_;
        }

        other.data_ = other.inline_;
        other.size_ = 0;
        other.capacity_ = Inline;
    }

    // Not initialized: the builder writes every byte that it adds
    alignas(8) std::uint8_t inline_[Inline];
    std::uint8_t* data_ = inline_;
    std::size_t size_ = 0;
    std::size_t capacity_ = Inline;
};

/// A run of bytes that a builder borrows or owns, which goes in the table
/// before the builder's own byte at `at`
struct Segment {
    std::size_t at;
    const std::uint8_t* data;
    std::size_t size;
};

/// The segments of a table, in the order of their positions: the first
/// `Inline` within the builder, and the others in a vector, which holds them
/// all once they don't fit, as `tafla::TableBuilder` keeps its first fields
class Segments {
public:
    static constexpr std::size_t Inline = 4;

    Segments() = default;

    Segments(Segments&& other) noexcept
        : inline_(other.inline_), spilled_(std::move(other.spilled_)), size_(std::exchange(other.size_, 0)) {
        other.spilled_ = {};
    }

    Segments& operator=(Segments&& other) noexcept {
        if (this != &other) {
            inline_ = other.inline_;
            spilled_ = std::move(other.spilled_);
            size_ = std::exchange(other.size_, 0);
            other.spilled_ = {};
        }
        return *this;
    }

    Segments(const Segments&) = delete;
    Segments& operator=(const Segments&) = delete;
    ~Segments() = default;

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] Segment* begin() noexcept {
        return !spilled() ? inline_.data() : spilled_.data();
    }

    [[nodiscard]] const Segment* begin() const noexcept {
        return !spilled() ? inline_.data() : spilled_.data();
    }

    [[nodiscard]] const Segment* end() const noexcept {
        return begin() + size_;
    }

    [[nodiscard]] Segment& operator[](std::size_t index) noexcept {
        return begin()[index];
    }

    /// Makes room for `count` more segments, so that adding them doesn't
    /// throw
    void reserve_more(std::size_t count) {
        const auto size = size_ + count;
        if (!spilled()) {
            if (size <= Inline) {
                return;
            }
            std::vector<Segment> spilled;
            spilled.reserve((std::max)(size, 2 * Inline));
            spilled.assign(inline_.begin(), inline_.begin() + static_cast<std::ptrdiff_t>((std::min)(size_, Inline)));
            spilled_ = std::move(spilled);
        } else if (size > spilled_.capacity()) {
            spilled_.reserve((std::max)(size, 2 * spilled_.capacity()));
        }
    }

    /// Adds a segment, for which there's room
    void push_back(const Segment& segment) noexcept {
        if (!spilled()) {
            inline_[size_] = segment;
        } else {
            spilled_.push_back(segment);
        }
        ++size_;
    }

    /// Removes the segments past the first `size`, keeping the vector's room
    void truncate(std::size_t size) noexcept {
        if (spilled()) {
            spilled_.resize(size);
        }
        size_ = size;
    }

    void clear() noexcept {
        spilled_.clear();
        size_ = 0;
    }

private:
    /// Whether the vector holds the segments, which it does once it has
    /// room, even for none
    [[nodiscard]] bool spilled() const noexcept {
        return spilled_.capacity() != 0;
    }

    std::array<Segment, Inline> inline_{};
    std::vector<Segment> spilled_;
    std::size_t size_ = 0;
};

/// Stores `value` in a slot of `offset_type`, little-endian
TAFLA_ALWAYS_INLINE void store_slot(std::uint8_t* at, std::uint64_t value, OffsetType offset_type) noexcept {
    switch (offset_type) {
        case OffsetType::uint8:
            *at = static_cast<std::uint8_t>(value);
            return;
        case OffsetType::uint16:
            tafla::detail::store(at, static_cast<std::uint16_t>(value));
            return;
        case OffsetType::uint32:
            break;
    }
    tafla::detail::store(at, static_cast<std::uint32_t>(value));
}

}  // namespace detail

/// A table to nest with `set_table()` or in `set_vector()`, as
/// `tafla::NestedTable` is for `tafla::TableBuilder`: a builder or bytes,
/// borrowed from a `const` reference or span, or owned from an rvalue
template <std::size_t SegmentBytes>
class BasicDirectNestedTable {
public:
    using Builder = BasicDirectTableBuilder<SegmentBytes>;

    // NOLINTBEGIN(google-explicit-constructor): each converts as an argument
    BasicDirectNestedTable(Builder&& builder) : table_(std::move(builder)) {}

    BasicDirectNestedTable(const Builder& builder) noexcept : table_(&builder) {}

    BasicDirectNestedTable(std::span<const std::uint8_t> bytes) noexcept : table_(bytes) {}

    BasicDirectNestedTable(const std::vector<std::uint8_t>& bytes) noexcept
        : table_(std::span<const std::uint8_t>(bytes)) {}

    BasicDirectNestedTable(const Bytes& bytes) noexcept : table_(bytes.span()) {}

    BasicDirectNestedTable(std::vector<std::uint8_t>&& bytes) noexcept : table_(std::move(bytes)) {}

    BasicDirectNestedTable(Bytes&& bytes) noexcept : table_(std::move(bytes)) {}

    BasicDirectNestedTable(const Builder&& builder) = delete;
    // NOLINTEND(google-explicit-constructor)

private:
    friend Builder;

    [[nodiscard]] const Builder* builder() const noexcept {
        if (const auto* borrowed = std::get_if<const Builder*>(&table_)) {
            return *borrowed;
        }
        return std::get_if<Builder>(&table_);
    }

    [[nodiscard]] std::span<const std::uint8_t> bytes() const noexcept {
        if (const auto* borrowed = std::get_if<std::span<const std::uint8_t>>(&table_)) {
            return *borrowed;
        }
        if (const auto* owned = std::get_if<Bytes>(&table_)) {
            return owned->span();
        }
        if (const auto* owned = std::get_if<std::vector<std::uint8_t>>(&table_)) {
            return *owned;
        }
        return {};
    }

    [[nodiscard]] std::size_t byte_length() const noexcept {
        const auto* table = builder();
        return table != nullptr ? table->byte_length() : bytes().size();
    }

    [[nodiscard]] Alignment default_alignment() const noexcept {
        const auto* table = builder();
        return table != nullptr ? table->alignment() : Alignment::four;
    }

    void check_complete() const {
        if (const auto* table = builder()) {
            table->check_complete();
        }
    }

    std::variant<const Builder*, Builder, std::span<const std::uint8_t>, Bytes, std::vector<std::uint8_t>> table_;
};

/// Builds a table as `tafla::TableBuilder` does, with the same setters,
/// bytes and errors, but writes each field's bytes when it's set.
///
/// The builder writes the table forward into bytes of its own, which hold
/// the first 128 bytes within the builder and grow on the heap, at least
/// twofold, past them. A field's offset is written when it's set, since it
/// follows from the bytes written before it; a sized table's length is
/// written by `to_bytes()`, which copies the bytes into new bytes of the
/// table's exact length.
///
/// Runs of `SegmentBytes` or more, an array's elements, a string's or a
/// table's bytes, aren't copied when they're set: the builder keeps them as
/// segments, which `to_bytes()` copies between its own bytes. A borrowed run
/// must then outlive the builder's last `to_bytes()`, as every borrowed value
/// must for `tafla::TableBuilder`; an owned one is kept until the builder is
/// destroyed.
///
/// A table nested in place with `set_table(fields, fill)` is written where
/// its parent is, without padding, and moved forward by its padding once its
/// fields are set, when its alignment is known. Its offsets count from its
/// own start, so moving it changes none of them.
///
/// A setter that throws leaves the builder as it was: it checks the field
/// before writing anything, and a nested table or vector whose `fill` throws
/// is removed by truncating the bytes, segments and owned values to what
/// they were.
template <std::size_t SegmentBytes>
class BasicDirectTableBuilder {
public:
    using NestedTable = BasicDirectNestedTable<SegmentBytes>;
    class Item;

    BasicDirectTableBuilder(std::size_t fields, TableLayout<false> layout)
        : BasicDirectTableBuilder(fields,
                                  tafla::detail::header_length(fields, layout.is_sized(), false, layout.offset_type()),
                                  layout.is_sized(), std::nullopt, layout.offset_type()) {}

    BasicDirectTableBuilder(std::size_t fields, TableLayout<true> layout, std::uint32_t version)
        : BasicDirectTableBuilder(fields, versioned_header(fields, layout, version), layout.is_sized(), version,
                                  layout.offset_type()) {}

    /// Moves `other`'s bytes and what it owns, and leaves it as a builder
    /// that was just started, with its layout and number of fields
    BasicDirectTableBuilder(BasicDirectTableBuilder&& other) noexcept
        : buffer_(std::move(other.buffer_)),
          segments_(std::move(other.segments_)),
          owned_(std::move(other.owned_)),
          shift_(other.shift_),
          table_(other.table_),
          slots_(other.slots_),
          fields_(other.fields_),
          set_(other.set_),
          alignment_(other.alignment_),
          version_(other.version_),
          versioned_(other.versioned_),
          sized_(other.sized_),
          offset_type_(other.offset_type_) {
        other.restart();
    }

    BasicDirectTableBuilder& operator=(BasicDirectTableBuilder&& other) noexcept {
        if (this != &other) {
            buffer_ = std::move(other.buffer_);
            segments_ = std::move(other.segments_);
            owned_ = std::move(other.owned_);
            shift_ = other.shift_;
            table_ = other.table_;
            slots_ = other.slots_;
            fields_ = other.fields_;
            set_ = other.set_;
            alignment_ = other.alignment_;
            version_ = other.version_;
            versioned_ = other.versioned_;
            sized_ = other.sized_;
            offset_type_ = other.offset_type_;
            other.restart();
        }
        return *this;
    }

    BasicDirectTableBuilder(const BasicDirectTableBuilder&) = delete;
    BasicDirectTableBuilder& operator=(const BasicDirectTableBuilder&) = delete;
    ~BasicDirectTableBuilder() = default;

    /// Copies the table into new bytes of its exact length, with its
    /// segments in their places and its length in a sized header
    [[nodiscard]] Bytes to_bytes() const {
        check_complete();

        auto bytes = Bytes::for_overwrite(position());
        copy_into(bytes.data());
        if (sized_) {
            detail::store_slot(bytes.data(), position(), offset_type_);
        }
        return bytes;
    }

    /// The table's byte length with the fields set so far
    [[nodiscard]] std::size_t byte_length() const noexcept {
        return position();
    }

    /// The largest alignment that a field needs from the table's start
    [[nodiscard]] Alignment alignment() const noexcept {
        return alignment_;
    }

    TAFLA_ALWAYS_INLINE void set_int8(std::int8_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint8(std::uint8_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_int16(std::int16_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint16(std::uint16_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_int32(std::int32_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint32(std::uint32_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_int64(std::int64_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint64(std::uint64_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_float32(float value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_float64(double value) {
        set_scalar(value);
    }

    void set_int8_array(std::span<const std::int8_t> values) {
        set_array(values);
    }

    void set_int8_array(std::vector<std::int8_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint8_array(std::span<const std::uint8_t> values) {
        set_array(values);
    }

    void set_uint8_array(std::vector<std::uint8_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_int16_array(std::span<const std::int16_t> values) {
        set_array(values);
    }

    void set_int16_array(std::vector<std::int16_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint16_array(std::span<const std::uint16_t> values) {
        set_array(values);
    }

    void set_uint16_array(std::vector<std::uint16_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_int32_array(std::span<const std::int32_t> values) {
        set_array(values);
    }

    void set_int32_array(std::vector<std::int32_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint32_array(std::span<const std::uint32_t> values) {
        set_array(values);
    }

    void set_uint32_array(std::vector<std::uint32_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_int64_array(std::span<const std::int64_t> values) {
        set_array(values);
    }

    void set_int64_array(std::vector<std::int64_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint64_array(std::span<const std::uint64_t> values) {
        set_array(values);
    }

    void set_uint64_array(std::vector<std::uint64_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_float32_array(std::span<const float> values) {
        set_array(values);
    }

    void set_float32_array(std::vector<float>&& values) {
        set_owned_array(std::move(values));
    }

    void set_float64_array(std::span<const double> values) {
        set_array(values);
    }

    void set_float64_array(std::vector<double>&& values) {
        set_owned_array(std::move(values));
    }

    void set_string(std::string_view string) {
        check_field(tafla::detail::saturating_sum(width(), string.size()));
        put_counted(0, string.size(), string.data(), string.size(), Alignment::one, nullptr);
    }

    void set_string(const char* string) {
        set_string(std::string_view(string));
    }

    void set_string(std::string&& string) {
        check_field(tafla::detail::saturating_sum(width(), string.size()));
        const auto kept = keep(std::move(string));
        put_counted(0, kept.value.size(), kept.value.data(), kept.value.size(), Alignment::one, kept.owner);
    }

    void set_struct(std::span<const std::uint8_t> bytes) {
        check_field(bytes.size());
        put_bytes(0, bytes.data(), bytes.size(), Alignment::one, nullptr);
    }

    void set_struct(std::vector<std::uint8_t>&& bytes) {
        check_field(bytes.size());
        const auto kept = keep(std::move(bytes));
        put_bytes(0, kept.value.data(), kept.value.size(), Alignment::one, kept.owner);
    }

    void set_struct_array(std::span<const std::uint8_t> bytes, std::size_t size, Alignment alignment) {
        const auto padding = check_struct_array(bytes.size(), size, alignment);
        put_counted(padding, bytes.size() / size, bytes.data(), bytes.size(), alignment, nullptr);
    }

    void set_struct_array(std::vector<std::uint8_t>&& bytes, std::size_t size, Alignment alignment) {
        const auto padding = check_struct_array(bytes.size(), size, alignment);
        const auto kept = keep(std::move(bytes));
        put_counted(padding, kept.value.size() / size, kept.value.data(), kept.value.size(), alignment, kept.owner);
    }

    void set_table(NestedTable table) {
        const auto alignment = table.default_alignment();
        set_table(std::move(table), alignment);
    }

    void set_table(NestedTable table, Alignment alignment);

    void set_vector(std::vector<NestedTable> items);

    template <std::invocable<Item&> Fill>
    TAFLA_ALWAYS_INLINE void set_table(std::size_t fields, Fill&& fill);

    template <std::invocable<std::size_t, Item&> Fill>
    void set_vector(std::size_t count, std::size_t fields, Fill&& fill);

private:
    friend NestedTable;

    using Owner = std::shared_ptr<const tafla::detail::Owned>;

    /// A value that the builder owns, and its owner, which a segment of it
    /// keeps alive
    template <class T>
    struct Kept {
        const T& value;
        const Owner* owner;
    };

    /// What the builder counts of the table that it's writing: its own, or
    /// one that it writes in place
    struct Counters {
        std::size_t table;
        std::size_t slots;
        std::uint32_t fields;
        std::uint32_t set;
        Alignment alignment;
    };

    /// What the builder holds, which a nested table or vector whose `fill`
    /// throws is truncated back to
    struct Checkpoint {
        Counters counters;
        std::size_t bytes;
        std::size_t shift;
        std::size_t segments;
        std::size_t owned;
    };

    BasicDirectTableBuilder(std::size_t fields, std::uint32_t header, bool sized, std::optional<std::uint32_t> version,
                            OffsetType offset_type)
        : fields_(static_cast<std::uint32_t>(fields)),
          version_(version.value_or(0)),
          versioned_(version.has_value()),
          sized_(sized),
          offset_type_(offset_type) {
        write_header(header);
    }

    static std::uint32_t versioned_header(std::size_t fields, TableLayout<true> layout, std::uint32_t version) {
        tafla::detail::check_version(version, layout.offset_type());
        return tafla::detail::header_length(fields, layout.is_sized(), true, layout.offset_type());
    }

    /// Writes the root's header: its length, written by `to_bytes()`, its
    /// version and its offsets, written as each field is set
    void write_header(std::uint32_t header) {
        auto* at = buffer_.extend(header);
        tafla::detail::zero(at, header);

        const std::size_t slots = (sized_ ? 1U : 0U) + (versioned_ ? 1U : 0U);
        if (versioned_) {
            detail::store_slot(at + (sized_ ? width() : 0), version_, offset_type_);
        }
        slots_ = slots * width();
    }

    /// Leaves the builder as its constructor did, without fields, after its
    /// bytes were moved
    void restart() noexcept {
        segments_.clear();
        owned_.clear();
        shift_ = 0;
        table_ = 0;
        set_ = 0;
        alignment_ = Alignment::one;

        const std::size_t slots = (sized_ ? 1U : 0U) + (versioned_ ? 1U : 0U);
        write_header(static_cast<std::uint32_t>((slots + fields_) * width()));
    }

    [[nodiscard]] std::size_t width() const noexcept {
        return offset_size(offset_type_);
    }

    /// The position in the table of the next byte: the bytes written and
    /// the segments before them
    [[nodiscard]] std::size_t position() const noexcept {
        return buffer_.size() + shift_;
    }

    /// The length of the table being written, which is where its next field
    /// goes
    [[nodiscard]] std::size_t table_length() const noexcept {
        return position() - table_;
    }

    [[nodiscard]] Counters counters() const noexcept {
        return {table_, slots_, fields_, set_, alignment_};
    }

    void restore(const Counters& counters) noexcept {
        table_ = counters.table;
        slots_ = counters.slots;
        fields_ = counters.fields;
        set_ = counters.set;
        alignment_ = counters.alignment;
    }

    [[nodiscard]] Checkpoint checkpoint() const noexcept {
        return {counters(), buffer_.size(), shift_, segments_.size(), owned_.size()};
    }

    void truncate(const Checkpoint& checkpoint) noexcept {
        restore(checkpoint.counters);
        buffer_.truncate(checkpoint.bytes);
        shift_ = checkpoint.shift;
        segments_.truncate(checkpoint.segments);
        owned_.erase(owned_.begin() + static_cast<std::ptrdiff_t>(checkpoint.owned), owned_.end());
    }

    void check_complete() const {
        if (set_ != fields_) {
            throw BuildError::incomplete(set_, fields_);
        }
    }

    /// Checks that the table has a field left to set, and that a field of
    /// `byte_length` bytes, its padding included, leaves it short enough
    TAFLA_ALWAYS_INLINE void check_field(std::uint64_t byte_length) const {
        if (set_ == fields_) [[unlikely]] {
            throw_too_many_fields(fields_);
        }
        check_length(tafla::detail::saturating_sum(table_length(), byte_length));
    }

    /// Checks that the table is short enough at `length`, after its field
    /// `set_`
    TAFLA_ALWAYS_INLINE void check_length(std::uint64_t length) const {
        if (length > max_value(offset_type_)) [[unlikely]] {
            throw_too_long(set_, length, offset_type_);
        }
    }

    [[noreturn]] TAFLA_COLD static void throw_too_many_fields(std::size_t fields) {
        throw BuildError::too_many_fields(fields);
    }

    [[noreturn]] TAFLA_COLD static void throw_too_long(std::size_t index, std::uint64_t length,
                                                       OffsetType offset_type) {
        throw BuildError::too_long(index, length, offset_type);
    }

    /// Checks an array of structs, and returns the padding before its count
    [[nodiscard]] std::size_t check_struct_array(std::size_t length, std::size_t size, Alignment alignment) const {
        if (size == 0 || length % size != 0) {
            throw BuildError::not_whole_structs(length, size);
        }

        check_field(tafla::detail::array_bytes(table_length(), width(), length, alignment));
        return tafla::detail::padding(table_length() + width(), alignment);
    }

    /// Writes the offset of the field being set, which starts at `position`
    TAFLA_ALWAYS_INLINE void write_offset(std::size_t position) noexcept {
        detail::store_slot(buffer_.data() + slots_ + std::size_t{set_} * width(), position - table_, offset_type_);
    }

    /// Counts the field just written, which needs `alignment` from the
    /// table's start
    TAFLA_ALWAYS_INLINE void commit(Alignment alignment) noexcept {
        ++set_;
        alignment_ = (std::max)(alignment_, alignment);
    }

    /// Whether a run of `size` bytes is kept as a segment rather than copied
    [[nodiscard]] static constexpr bool segments(std::size_t size) noexcept {
        return size >= SegmentBytes;
    }

    /// Makes room for a segment, so that adding it doesn't throw
    void reserve_segments(std::size_t count) {
        segments_.reserve_more(count);
    }

    void reserve_owned(std::size_t count) {
        const auto size = owned_.size() + count;
        if (size > owned_.capacity()) {
            owned_.reserve((std::max<std::size_t>)({size, 2 * owned_.capacity(), 4}));
        }
    }

    /// Keeps `value` until the builder is destroyed, if it's long enough to
    /// be a segment, so that its bytes needn't be copied when it's set. A
    /// shorter value is copied, and left to the caller.
    template <class T>
    Kept<T> keep(T&& value) {
        const auto size = value.size() * sizeof(typename T::value_type);
        if (!segments(size)) {
            return {value, nullptr};
        }

        reserve_owned(1);
        auto owned = std::make_shared<tafla::detail::OwnedValue<T>>(std::move(value));
        const T& kept = owned->value;
        owned_.push_back(std::move(owned));
        return {kept, &owned_.back()};
    }

    /// Writes a field of `size` bytes from `data` after `padding` zeros, and
    /// its offset, which points past the padding. The setter checked it, and
    /// kept its value if it's owned, which this removes if growing the
    /// bytes fails.
    void put_bytes(std::size_t padding, const void* data, std::size_t size, Alignment alignment, const Owner* owner) {
        const auto start = position() + padding;
        if (segments(size)) {
            reserve_or_forget(owner, [&] {
                reserve_segments(1);
                return buffer_.extend(padding);
            });
            tafla::detail::zero(buffer_.data() + buffer_.size() - padding, padding);
            write_offset(start);
            add_segment(data, size);
        } else {
            auto* at = reserve_or_forget(owner, [&] { return buffer_.extend(padding + size); });
            tafla::detail::zero(at, padding);
            write_offset(start);
            tafla::detail::copy(at + padding, data, size);
        }
        commit(alignment);
    }

    /// Writes a field of a count, `count`, and then `size` bytes from `data`,
    /// after `padding` zeros, as `put_bytes()` writes them
    void put_counted(std::size_t padding, std::size_t count, const void* data, std::size_t size, Alignment alignment,
                     const Owner* owner) {
        const auto start = position() + padding;
        std::uint8_t* at = nullptr;
        if (segments(size)) {
            at = reserve_or_forget(owner, [&] {
                reserve_segments(1);
                return buffer_.extend(padding + width());
            });
        } else {
            at = reserve_or_forget(owner, [&] { return buffer_.extend(padding + width() + size); });
        }

        tafla::detail::zero(at, padding);
        write_offset(start);
        detail::store_slot(at + padding, count, offset_type_);
        if (segments(size)) {
            add_segment(data, size);
        } else {
            tafla::detail::copy(at + padding + width(), data, size);
        }
        commit(alignment);
    }

    /// Calls `reserve`, which makes room for a field, and removes the owned
    /// value of the field if it throws
    template <class Reserve>
    TAFLA_ALWAYS_INLINE std::uint8_t* reserve_or_forget(const Owner* owner, Reserve reserve) {
        if (owner == nullptr) {
            return reserve();
        }
        try {
            return reserve();
        } catch (...) {
            owned_.pop_back();
            throw;
        }
    }

    /// Adds a segment of `size` bytes at `data`, for which there's room,
    /// before the next byte written
    void add_segment(const void* data, std::size_t size) noexcept {
        segments_.push_back({buffer_.size(), static_cast<const std::uint8_t*>(data), size});
        shift_ += size;
    }

    template <class T>
    TAFLA_ALWAYS_INLINE void set_scalar(T value) {
        check_field(sizeof(T));

        const auto start = position();
        auto* at = buffer_.extend(sizeof(T));
        write_offset(start);
        tafla::detail::store(at, value);
        commit(Alignment::one);
    }

    template <class T>
    void set_array(std::span<const T> values) {
        constexpr auto alignment = static_cast<Alignment>(sizeof(T));
        const auto elements = tafla::detail::saturating_product(values.size(), sizeof(T));
        check_field(tafla::detail::array_bytes(table_length(), width(), elements, alignment));

        const auto padding = tafla::detail::padding(table_length() + width(), alignment);
        if constexpr (tafla::detail::kHostOrder || sizeof(T) == 1) {
            put_counted(padding, values.size(), values.data(), values.size_bytes(), alignment, nullptr);
        } else {
            put_swapped(padding, values);
        }
    }

    template <class T>
    void set_owned_array(std::vector<T>&& values) {
        if constexpr (tafla::detail::kHostOrder || sizeof(T) == 1) {
            constexpr auto alignment = static_cast<Alignment>(sizeof(T));
            const auto elements = tafla::detail::saturating_product(values.size(), sizeof(T));
            check_field(tafla::detail::array_bytes(table_length(), width(), elements, alignment));

            const auto padding = tafla::detail::padding(table_length() + width(), alignment);
            const auto kept = keep(std::move(values));
            put_counted(padding, kept.value.size(), kept.value.data(), kept.value.size() * sizeof(T), alignment,
                        kept.owner);
        } else {
            set_array(std::span<const T>(values));
        }
    }

    /// Writes an array whose elements are swapped to little-endian one by
    /// one, on a big-endian host
    template <class T>
    void put_swapped(std::size_t padding, std::span<const T> values) {
        const auto start = position() + padding;
        auto* at = buffer_.extend(padding + width() + values.size_bytes());
        tafla::detail::zero(at, padding);
        write_offset(start);
        detail::store_slot(at + padding, values.size(), offset_type_);

        auto* element = at + padding + width();
        for (const T value : values) {
            tafla::detail::store(element, value);
            element += sizeof(T);
        }
        commit(static_cast<Alignment>(sizeof(T)));
    }

    /// The bytes and segments that appending `table` adds, for which
    /// `append_nested()` needs room
    struct Room {
        std::size_t bytes = 0;
        std::size_t segments = 0;
        std::size_t owned = 0;

        void add(const NestedTable& table) noexcept {
            if (const auto* builder = table.builder()) {
                bytes += builder->buffer_.size();
                segments += builder->segments_.size();
                owned += builder->owned_.size();
            } else if (BasicDirectTableBuilder::segments(table.bytes().size())) {
                ++segments;
                owned += std::holds_alternative<std::span<const std::uint8_t>>(table.table_) ? 0 : 1;
            } else {
                bytes += table.bytes().size();
            }
        }
    };

    void reserve(const Room& room) {
        reserve_segments(room.segments);
        reserve_owned(room.owned);
        buffer_.reserve_more(room.bytes);
    }

    /// Appends a nested table's bytes, for which `reserve()` made room: a
    /// builder's bytes and segments, with its length in a sized header, or
    /// bytes, kept as a segment when they're long enough. It takes what a
    /// builder owns from an rvalue, and shares it from a `const` reference.
    /// Only keeping owned bytes allocates, which `set_table()` and
    /// `set_vector()` truncate after.
    void append_nested(NestedTable& table) {
        if (const auto* builder = table.builder()) {
            append_builder(*builder);
            if (auto* owned = std::get_if<BasicDirectTableBuilder>(&table.table_)) {
                std::move(owned->owned_.begin(), owned->owned_.end(), std::back_inserter(owned_));
            } else {
                owned_.insert(owned_.end(), builder->owned_.begin(), builder->owned_.end());
            }
            return;
        }

        const auto bytes = table.bytes();
        if (!segments(bytes.size())) {
            tafla::detail::copy(buffer_.extend(bytes.size()), bytes.data(), bytes.size());
            return;
        }

        if (auto* owned = std::get_if<Bytes>(&table.table_)) {
            owned_.push_back(std::make_shared<tafla::detail::OwnedValue<Bytes>>(std::move(*owned)));
        } else if (auto* owned_vector = std::get_if<std::vector<std::uint8_t>>(&table.table_)) {
            owned_.push_back(
                std::make_shared<tafla::detail::OwnedValue<std::vector<std::uint8_t>>>(std::move(*owned_vector)));
        }
        add_segment(bytes.data(), bytes.size());
    }

    /// Appends the bytes and segments of a complete builder
    void append_builder(const BasicDirectTableBuilder& builder) noexcept {
        const auto start = buffer_.size();
        auto* at = buffer_.extend(builder.buffer_.size());
        std::memcpy(at, builder.buffer_.data(), builder.buffer_.size());
        if (builder.sized_) {
            detail::store_slot(at, builder.position(), builder.offset_type_);
        }

        for (const auto& segment : builder.segments_) {
            segments_.push_back({start + segment.at, segment.data, segment.size});
        }
        shift_ += builder.shift_;
    }

    /// Makes room, after the first of a vector's `count` items wrote `first`
    /// bytes, for the others: as many bytes each as the first's, so that
    /// items alike, as a vector's usually are, grow the bytes at most once,
    /// but for no more than the table's offsets address. Unlike the room that
    /// `tafla::TableBuilder` makes for its items' records, it isn't bounded by
    /// the least that the others write, their headers, which left the
    /// comparison's items growing twice more; a first item much longer than
    /// the others leaves room unused until the builder is destroyed.
    void reserve_items_like(std::size_t first, std::size_t count) {
        const auto like_first = tafla::detail::saturating_product(count - 1, first);
        buffer_.reserve_more(static_cast<std::size_t>((std::min<std::uint64_t>)(like_first, max_value(offset_type_))));
    }

    /// Starts a table of `fields` fields in place, whose header of `header`
    /// bytes holds only their offsets, at the next byte, and counts its
    /// fields rather than its parent's
    TAFLA_ALWAYS_INLINE void start_in_place(std::uint32_t fields, std::uint32_t header) {
        const auto start = position();
        auto* at = buffer_.extend(header);
        tafla::detail::zero(at, header);
        restore({start, buffer_.size() - header, fields, 0, Alignment::one});
    }

    /// Moves the table written in place since `saved` forward by `padding`,
    /// with its segments, and zeros the padding before it
    [[gnu::noinline]] void move_by(std::size_t padding, const Checkpoint& saved) {
        const auto length = buffer_.size() - saved.bytes;
        buffer_.extend(padding);
        auto* table = buffer_.data() + saved.bytes;
        std::memmove(table + padding, table, length);
        tafla::detail::zero(table, padding);
        for (auto segment = saved.segments; segment < segments_.size(); ++segment) {
            segments_[segment].at += padding;
        }
    }

    /// Copies the table's bytes, with its segments in their places, to `to`
    void copy_into(std::uint8_t* to) const noexcept {
        std::size_t from = 0;
        for (const auto& segment : segments_) {
            const auto before = segment.at - from;
            tafla::detail::copy(to, buffer_.data() + from, before);
            to += before;
            tafla::detail::copy(to, segment.data, segment.size);
            to += segment.size;
            from = segment.at;
        }
        tafla::detail::copy(to, buffer_.data() + from, buffer_.size() - from);
    }

    detail::Buffer buffer_;
    /// The runs of bytes kept rather than copied, in the order of their
    /// positions
    detail::Segments segments_;
    /// The values that segments point into, which builders nested by
    /// reference share
    std::vector<Owner> owned_;
    /// The bytes of the segments, by which the table's positions are past
    /// the buffer's
    std::size_t shift_ = 0;
    /// The position of the table being written, which its offsets count
    /// from: 0 for the root
    std::size_t table_ = 0;
    /// The position in the buffer of that table's first offset
    std::size_t slots_ = 0;
    std::uint32_t fields_;
    std::uint32_t set_ = 0;
    Alignment alignment_ = Alignment::one;
    std::uint32_t version_;
    bool versioned_;
    bool sized_;
    OffsetType offset_type_;
};

/// The fields of a table that a builder writes in place, as
/// `tafla::TableBuilder::Item` is
template <std::size_t SegmentBytes>
class BasicDirectTableBuilder<SegmentBytes>::Item {
public:
    Item(const Item&) = delete;
    Item& operator=(const Item&) = delete;
    ~Item() = default;

    TAFLA_ALWAYS_INLINE void set_int8(std::int8_t value) {
        builder_->set_int8(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint8(std::uint8_t value) {
        builder_->set_uint8(value);
    }

    TAFLA_ALWAYS_INLINE void set_int16(std::int16_t value) {
        builder_->set_int16(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint16(std::uint16_t value) {
        builder_->set_uint16(value);
    }

    TAFLA_ALWAYS_INLINE void set_int32(std::int32_t value) {
        builder_->set_int32(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint32(std::uint32_t value) {
        builder_->set_uint32(value);
    }

    TAFLA_ALWAYS_INLINE void set_int64(std::int64_t value) {
        builder_->set_int64(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint64(std::uint64_t value) {
        builder_->set_uint64(value);
    }

    TAFLA_ALWAYS_INLINE void set_float32(float value) {
        builder_->set_float32(value);
    }

    TAFLA_ALWAYS_INLINE void set_float64(double value) {
        builder_->set_float64(value);
    }

    template <class Values>
    void set_int8_array(Values&& values) {
        builder_->set_int8_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_uint8_array(Values&& values) {
        builder_->set_uint8_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_int16_array(Values&& values) {
        builder_->set_int16_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_uint16_array(Values&& values) {
        builder_->set_uint16_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_int32_array(Values&& values) {
        builder_->set_int32_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_uint32_array(Values&& values) {
        builder_->set_uint32_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_int64_array(Values&& values) {
        builder_->set_int64_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_uint64_array(Values&& values) {
        builder_->set_uint64_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_float32_array(Values&& values) {
        builder_->set_float32_array(std::forward<Values>(values));
    }

    template <class Values>
    void set_float64_array(Values&& values) {
        builder_->set_float64_array(std::forward<Values>(values));
    }

    template <class String>
    void set_string(String&& string) {
        builder_->set_string(std::forward<String>(string));
    }

    template <class Range>
    void set_struct(Range&& bytes) {
        builder_->set_struct(std::forward<Range>(bytes));
    }

    template <class Range>
    void set_struct_array(Range&& bytes, std::size_t size, Alignment alignment) {
        builder_->set_struct_array(std::forward<Range>(bytes), size, alignment);
    }

    void set_table(NestedTable table) {
        builder_->set_table(std::move(table));
    }

    void set_table(NestedTable table, Alignment alignment) {
        builder_->set_table(std::move(table), alignment);
    }

    template <std::invocable<Item&> Fill>
    TAFLA_ALWAYS_INLINE void set_table(std::size_t fields, Fill&& fill) {
        builder_->set_table(fields, std::forward<Fill>(fill));
    }

    void set_vector(std::vector<NestedTable> items) {
        builder_->set_vector(std::move(items));
    }

    template <std::invocable<std::size_t, Item&> Fill>
    void set_vector(std::size_t count, std::size_t fields, Fill&& fill) {
        builder_->set_vector(count, fields, std::forward<Fill>(fill));
    }

private:
    friend class BasicDirectTableBuilder;

    explicit Item(BasicDirectTableBuilder& builder) noexcept : builder_(&builder) {}

    BasicDirectTableBuilder* builder_;
};

template <std::size_t SegmentBytes>
void BasicDirectTableBuilder<SegmentBytes>::set_table(NestedTable table, Alignment alignment) {
    table.check_complete();

    const std::size_t padding = tafla::detail::padding(table_length(), alignment);
    check_field(tafla::detail::saturating_sum(padding, table.byte_length()));

    Room room;
    room.bytes = padding;
    room.add(table);
    reserve(room);

    const auto saved = checkpoint();
    try {
        tafla::detail::zero(buffer_.extend(padding), padding);
        write_offset(position());
        append_nested(table);
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(alignment);
}

template <std::size_t SegmentBytes>
void BasicDirectTableBuilder<SegmentBytes>::set_vector(std::vector<NestedTable> items) {
    std::uint64_t nested_bytes = 0;
    Room room;
    for (const auto& item : items) {
        item.check_complete();
        nested_bytes = tafla::detail::saturating_sum(nested_bytes, item.byte_length());
        room.add(item);
    }

    const auto slots = tafla::detail::vector_bytes(items.size(), width());
    check_field(tafla::detail::saturating_sum(slots, nested_bytes));

    room.bytes += static_cast<std::size_t>(slots);
    reserve(room);

    const auto saved = checkpoint();
    try {
        const auto vector = position();
        const auto ends = buffer_.size() + width();
        write_offset(vector);
        auto* at = buffer_.extend(static_cast<std::size_t>(slots));
        detail::store_slot(at, items.size(), offset_type_);
        tafla::detail::zero(at + width(), static_cast<std::size_t>(slots) - width());

        for (std::size_t index = 0; index < items.size(); ++index) {
            append_nested(items[index]);
            detail::store_slot(buffer_.data() + ends + index * width(), position() - vector, offset_type_);
        }
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(Alignment::one);
}

template <std::size_t SegmentBytes>
template <std::invocable<typename BasicDirectTableBuilder<SegmentBytes>::Item&> Fill>
TAFLA_ALWAYS_INLINE void BasicDirectTableBuilder<SegmentBytes>::set_table(std::size_t fields, Fill&& fill) {
    if (set_ == fields_) [[unlikely]] {
        throw_too_many_fields(fields_);
    }
    const auto header = tafla::detail::header_length(fields, false, false, offset_type_);

    const auto saved = checkpoint();
    try {
        start_in_place(static_cast<std::uint32_t>(fields), header);
        Item item(*this);
        std::invoke(fill, item);
        check_complete();

        // Padded from the parent's length where the table started, as a
        // nested builder is, which moves the table, whose offsets count
        // from its own start, forward by the padding
        const auto start = table_;
        const auto alignment = alignment_;
        restore(saved.counters);
        const std::size_t padding = tafla::detail::padding(start - table_, alignment);
        check_length(tafla::detail::saturating_sum(table_length(), padding));

        if (padding != 0) [[unlikely]] {
            move_by(padding, saved);
        }

        write_offset(start + padding);
        commit(alignment);
    } catch (...) {
        truncate(saved);
        throw;
    }
}

template <std::size_t SegmentBytes>
template <std::invocable<std::size_t, typename BasicDirectTableBuilder<SegmentBytes>::Item&> Fill>
void BasicDirectTableBuilder<SegmentBytes>::set_vector(std::size_t count, std::size_t fields, Fill&& fill) {
    // The count and the items' ends, which fail before any item is written
    const auto slots = tafla::detail::vector_bytes(count, width());
    check_field(slots);
    const auto header = tafla::detail::header_length(fields, false, false, offset_type_);

    const auto saved = checkpoint();
    try {
        const auto vector = position();
        const auto ends = buffer_.size() + width();
        auto* at = buffer_.extend(static_cast<std::size_t>(slots));
        write_offset(vector);
        detail::store_slot(at, count, offset_type_);
        tafla::detail::zero(at + width(), static_cast<std::size_t>(slots) - width());

        for (std::size_t index = 0; index < count; ++index) {
            const auto item_start = buffer_.size();
            start_in_place(static_cast<std::uint32_t>(fields), header);
            Item item(*this);
            std::invoke(fill, index, item);
            check_complete();
            restore(saved.counters);

            detail::store_slot(buffer_.data() + ends + index * width(), position() - vector, offset_type_);

            if (index == 0 && count > 1) {
                reserve_items_like(buffer_.size() - item_start, count);
            }
        }

        check_length(table_length());
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(Alignment::one);
}

}  // namespace tafla::prototype

#endif
