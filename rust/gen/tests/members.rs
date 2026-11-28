//! Fields named after a member that a target's code defines or inherits, or
//! a local that it writes them beside, take a `_` after their name in that
//! target, as keywords do, so that the code compiles and keeps its members.
//! Targets where the name doesn't clash keep it.

use tafla_gen::{Options, Target, compile};

const SCHEMA: &str = "\
message Members (version: 1) {
    constructor: uint8;
    to_string: uint8;
    serialize: uint8;
    new: uint8;
    builder: uint8;
    cls: uint8;
    int: uint8;
    bool: bool;
    equals: uint8;
    read: uint8;
    write: uint8;
    then: uint8;
    version_of: uint8;
}

struct Frame {
    serialize: uint8;
    writer: uint8;
    bytes: uint8;
    read: uint8;
}
";

fn generate(target: &str, schema: &str) -> String {
    let target = Target::from_name(target).unwrap();
    let files = compile("members.tafla", schema, target, &Options::default()).unwrap();

    files.into_iter().map(|file| file.contents).collect()
}

fn assert_declares(target: &str, declarations: &[&str]) {
    assert_generates(target, SCHEMA, declarations);
}

fn assert_generates(target: &str, schema: &str, declarations: &[&str]) {
    let code = generate(target, schema);
    for declaration in declarations {
        assert!(
            code.contains(declaration),
            "{target} doesn't declare `{declaration}`:\n{code}"
        );
    }
}

#[test]
fn typescript_renames_object_members_and_then() {
    assert_declares(
        "typescript",
        &[
            "    constructor_(): number {",
            "    toString_(): number {",
            "    then_(): number {",
            "    constructor_: number;",
            "    declare private readonly _writer: number;",
            "    serialize(): number {",
            "    new(): number {",
            "    static versionOf(bytes: Uint8Array): number {",
            "    versionOf(): number {",
        ],
    );
}

#[test]
fn javascript_renames_object_members_and_then() {
    assert_declares(
        "javascript",
        &[
            "    constructor_() {",
            "    toString_() {",
            "    then_() {",
            " * @property {number} constructor_",
            "    serialize() {",
        ],
    );
}

/// A TypeScript or JavaScript table reads every field with `fields()`, so a
/// field named `fields` takes a `_`
#[test]
fn typescript_and_javascript_rename_fields() {
    let schema = "message Form {\n    fields: uint8;\n}\n";
    assert_generates(
        "typescript",
        schema,
        &[
            "    fields_(): number {",
            "    fields(): FormFields {",
            "    fields_: number;",
        ],
    );
    assert_generates(
        "javascript",
        schema,
        &[
            "    fields_() {",
            "    fields() {",
            " * @property {number} fields_",
        ],
    );
}

/// A TypeScript or JavaScript view reads a field of its items with a getter
/// of the field's name, so fields named after its own `length()`, `get()` and
/// `fields()` take a `_` there, and keep their names in the item's reader
#[test]
fn typescript_views_rename_their_own_members() {
    let schema = "\
message Lists {
    entries: [Entry];
    spans: [Span];
}

table Entry {
    length: uint8;
    get: uint8;
}

struct Span {
    length: uint32;
    get: uint32;
    fields: uint32;
}
";
    assert_generates(
        "typescript",
        schema,
        &[
            "    length_(index: number): number {",
            "    get_(index: number): number {",
            "    fields_(index: number): number {",
            // A table's own getters, which a vector's view doesn't repeat
            "    get(): number {",
            "    length(): number {",
            "    get(index: number): Entry {",
            "    get(index: number): Span {",
            "    fields(): SpanFields[] {",
        ],
    );
}

#[test]
fn python_renames_methods_the_parameter_of_serialize_and_builtins() {
    assert_declares(
        "python",
        &[
            "    def serialize_(self) -> int:",
            "    def builder(self) -> int:",
            "    def write_(self) -> int:",
            "    def cls_(self) -> int:",
            "    def int_(self) -> int:",
            "    def bool_(self) -> bool:",
            "    def read_(self) -> int:",
            "    serialize_: int\n",
            "    cls_: int\n",
            "    def constructor(self) -> int:",
            "    def then(self) -> int:",
            "    def version_of(cls, data: Buffer) -> int:",
            "    def version_of_(self) -> int:",
        ],
    );
}

/// A Python struct's protocol declares each field as a property through
/// `_builtins.property`, which no field's name hides from the fields after it,
/// so fields named `property` and `builtins` keep their names
#[test]
fn python_struct_protocols_keep_their_decorator_whatever_the_fields_names() {
    let schema =
        "struct Flags {\n    property: uint8;\n    builtins: uint8;\n    after: uint8;\n}\n";
    assert_generates(
        "python",
        schema,
        &[
            "import builtins as _builtins\n",
            "    property: int\n",
            "    builtins: int\n",
            "    @_builtins.property\n    def property(self) -> int: ...",
            "    @_builtins.property\n    def builtins(self) -> int: ...",
            "    @_builtins.property\n    def after(self) -> int: ...",
        ],
    );
}

/// Python's structs set their slots through setters at module level, whose
/// names no field, declaration or other struct's setter takes, and their
/// `__init__()` names the instance `_self` when a field takes `self`
#[test]
fn python_structs_set_their_slots_through_names_that_nothing_else_takes() {
    let schema = "\
struct Frame {
    serialize: uint8;
    frame_self: uint8;
}

struct FrameSelf {
    self: uint8;
    frame: Frame;
}
";
    assert_generates(
        "python",
        schema,
        &[
            "    def __init__(self, serialize_: int, frame_self: int) -> None:",
            "        _Frame_serialize_(self, serialize_)\n",
            "        _Frame_frame_self(self, frame_self)\n",
            "_Frame_serialize_ = Frame.__dict__[\"serialize_\"].__set__\n",
            "_Frame_frame_self = Frame.__dict__[\"frame_self\"].__set__\n",
            "    def __init__(_self, self: int, frame: Frame) -> None:",
            "        _FrameSelf_self(_self, self)\n",
            "        _FrameSelf_frame(_self, frame)\n",
            "_FrameSelf_self = FrameSelf.__dict__[\"self\"].__set__\n",
            "_FrameSelf_frame = FrameSelf.__dict__[\"frame\"].__set__\n",
        ],
    );
}

#[test]
fn java_renames_object_methods_and_the_members_of_structs() {
    assert_declares(
        "java",
        &[
            "    public short toString_() {",
            "    public short serialize_() {",
            "public record Frame(short serialize_, short writer, short bytes_, short read) {",
            "    public short equals() {",
            "    public short builder() {",
            "    public short read() {",
            "    public short then() {",
            "    public static long versionOf(byte[] bytes) {",
            "    public short versionOf() {",
        ],
    );
}

#[test]
fn rust_renames_the_associated_functions_of_views() {
    assert_declares(
        "rust",
        &[
            "    pub fn new_(&self) -> Result<u8, DecodeError> {",
            "    pub new_: u8,",
            "    pub fn serialize(&self) -> Result<u8, DecodeError> {",
            "    pub fn read(&self) -> Result<u8, DecodeError> {",
            "    pub serialize: u8,",
            "    pub fn then(&self) -> Result<u8, DecodeError> {",
            "    pub fn version_of(bytes: &[u8]) -> Result<u32, DecodeError> {",
            "    pub fn version_of_(&self) -> Result<u8, DecodeError> {",
        ],
    );
}

#[test]
fn go_renames_the_binary_marshaling_methods_fields_and_the_methods_that_vet_checks() {
    let schema = "\
message Members (version: 1) {
    marshal_binary: uint8;
    append_binary: uint8;
    format: uint8;
    read: uint8;
    new: uint8;
    version_of: uint8;
    fields: uint8;
}

struct Frame {
    unmarshal_binary: uint8;
    read_byte: uint8;
    write_to: uint8;
    put: uint8;
    bytes: uint8;
}

table Encodings {
    marshal_j_s_o_n: uint8;
    unmarshal_x_m_l: uint8;
    marshal_json: uint8;
}
";

    assert_generates(
        "go",
        schema,
        &[
            "func (t Members) MarshalBinary_() (uint8, error) {",
            "func (t Members) AppendBinary_() (uint8, error) {",
            "func (t Members) Format_() (uint8, error) {",
            "func (t Members) Read() (uint8, error) {",
            "func (t Members) New() (uint8, error) {",
            "func (t Members) VersionOf() (uint8, error) {",
            "func MembersVersionOf(bytes []byte) (uint32, error) {",
            "func (t Members) Fields_() (uint8, error) {",
            "func (t Members) Fields() (MembersFields, error) {",
            "\tMarshalBinary_ uint8\n",
            "func (f MembersFields) MarshalBinary() ([]byte, error) {",
            "\tUnmarshalBinary_ uint8\n",
            "\tReadByte_        uint8\n",
            "\tWriteTo_         uint8\n",
            "\tPut              uint8\n",
            "\tBytes            uint8\n",
            "func (s *Frame) UnmarshalBinary(bytes []byte) error {",
            "\ts.Put = bytes[3]\n",
            // Fields whose exported names are those of methods that vet
            // checks take a `_`, and a name that only resembles one doesn't
            "func (t Encodings) MarshalJSON_() (uint8, error) {",
            "func (t Encodings) UnmarshalXML_() (uint8, error) {",
            "func (t Encodings) MarshalJson() (uint8, error) {",
        ],
    );
}

#[test]
fn cpp_renames_the_members_of_structs_and_fields_structs_and_keywords() {
    assert_declares(
        "cpp",
        &[
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t serialize_() const;",
            "    std::uint8_t serialize_{};",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t read_() const;",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t write_() const;",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t new_() const;",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t int_() const;",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE bool bool_() const;",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t then() const;",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t constructor() const;",
            "    [[nodiscard]] static std::uint32_t version_of(std::span<const std::uint8_t> bytes);",
            "    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t version_of_() const;",
            "    std::uint8_t writer{};",
            "    std::uint8_t bytes{};",
            "    std::uint8_t read_{};",
        ],
    );
}

/// Swift's fields structs and structs have `serialize()`, which a property
/// of its name clashes with, and a field named `self` would hide the instance
/// in an initializer, so both take a `_`, where other keywords are escaped in
/// backticks. A message or table holds its reader and position in
/// properties whose names start with a `_`, which no field's does.
#[test]
fn swift_renames_serialize_and_self() {
    let schema = "\
message Members (version: 1) {
    reader: uint8;
    serialize: uint8;
    version: uint8;
    format_version: uint8;
    layout: uint8;
    class: uint8;
    write: uint8;
    nested: Nested;
}

table Nested {
    position: uint8;
    self: uint8;
    alignment: uint8;
    add: uint8;
}

struct Frame {
    serialize: uint8;
    size: uint8;
    bytes: uint8;
    append: uint8;
}
";

    assert_generates(
        "swift",
        schema,
        &[
            "    public var reader: UInt8 {",
            "    let _reader: TableReader\n",
            "    public var serialize_: UInt8 {",
            "    public func serialize() throws(BuildError) -> [UInt8] {",
            "    public static func version(of bytes: RawSpan) throws(DecodeError) -> UInt32 {",
            "    public var version: UInt8 {",
            "    public static let formatVersion: UInt32 = 1\n",
            "    public var formatVersion: UInt8 {",
            "    public var layout: UInt8 {",
            "    public var `class`: UInt8 {",
            "        self.`class` = `class`\n",
            "    public var write: UInt8 {",
            "    public var position: UInt8 {",
            "    let _position: Int\n",
            "    public var self_: UInt8 {",
            "        self.self_ = self_\n",
            "    public var alignment: UInt8\n",
            "    public static let alignment: Alignment = .one\n",
            "    public var add: UInt8\n",
            "    public var size: UInt8\n",
            "    public static let size = 4\n",
            "    public var bytes: UInt8\n",
            "        self.bytes = UInt8(littleEndianBytes: bytes, at: offset + 2)\n",
            "    public var append: UInt8\n",
            "    public func serialize() -> [UInt8] {",
        ],
    );
}

#[test]
fn csharp_renames_the_members_of_readers_fields_and_structs_and_their_own_names() {
    assert_declares(
        "csharp",
        &[
            "    public byte ToString_ => _reader.GetUInt8(_reader.RootOffset(1));",
            "    public byte Serialize_ => _reader.GetUInt8(_reader.RootOffset(2));",
            "    public byte Equals_ => _reader.GetUInt8(_reader.RootOffset(8));",
            "    public byte Write_ => _reader.GetUInt8(_reader.RootOffset(10));",
            "    public byte VersionOf_ => _reader.GetUInt8(_reader.RootOffset(12));",
            "    public required byte Serialize_ { get; init; }",
            "    public static uint VersionOf(global::System.ReadOnlySpan<byte> bytes) =>",
            "    public byte[] Serialize()",
            // PascalCase names are never C#'s keywords, nor Python's or
            // TypeScript's members
            "    public byte Constructor => _reader.GetUInt8(_reader.RootOffset(0));",
            "    public byte New => _reader.GetUInt8(_reader.RootOffset(3));",
            "    public byte Int => _reader.GetUInt8(_reader.RootOffset(6));",
            "    public bool Bool => _reader.GetUInt8(_reader.RootOffset(7)) != 0;",
            "    public byte Read => _reader.GetUInt8(_reader.RootOffset(9));",
            "    public byte Then => _reader.GetUInt8(_reader.RootOffset(11));",
            "    public required byte Writer { get; init; }",
            "    public required byte Bytes { get; init; }",
            "    public required byte Read { get; init; }",
        ],
    );

    // A field named after its own definition, or its definition's fields,
    // which C# reserves for their constructors, and after the members of a
    // struct and of a table's fields
    assert_generates(
        "csharp",
        "message Point { point: uint8; point_fields: uint8; inner: Inner; whole: Whole; }\n\
         table Inner { add_to: uint8; write_to: uint8; name: string; }\n\
         table Whole { put_to: uint8; }\n\
         struct Frame { size: uint8; alignment: uint8; serialize_array: uint8; frame: uint8; }",
        &[
            "    public byte Point_ => _reader.GetUInt8(_reader.RootOffset(0));",
            "    public required byte PointFields_ { get; init; }",
            "    public required byte AddTo_ { get; init; }",
            "    internal void AddTo(ref global::Tafla.TableSizer sizer)",
            "    public required byte WriteTo_ { get; init; }",
            "    public required byte PutTo_ { get; init; }",
            "    internal void PutTo(global::System.Span<byte> bytes)",
            "    public const int Size = 4;",
            "    public required byte Size_ { get; init; }",
            "    public required byte Alignment_ { get; init; }",
            "    public required byte SerializeArray_ { get; init; }",
            "    public required byte Frame_ { get; init; }",
        ],
    );
}

/// C++'s enum values take a `_` after a keyword or a macro, as fields do, but
/// keep the names of the members that the generated code declares, since an
/// enum class declares nothing beside its values
#[test]
fn cpp_renames_enum_values_named_after_keywords_and_macros_only() {
    assert_generates(
        "cpp",
        "enum Member: uint8 { new = 0; linux = 1; read = 2; size = 3; then = 4; }",
        &[
            "enum class Member : std::uint8_t {",
            "    new_ = 0,",
            "    linux_ = 1,",
            "    read = 2,",
            "    size = 3,",
            "    then = 4,",
        ],
    );
}
