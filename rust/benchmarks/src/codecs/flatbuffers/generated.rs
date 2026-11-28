// Written by hand in the shape that flatc 25.12.19 generates with `--rust`,
// with only the accessors, builders and verifiers that the benchmarks use,
// from this schema:
//
//     table Record { x: double; y: double; id: long; name: string; }
//
//     table Columns { base: long; keys: [uint]; a: [float]; b: [float]; }
//     table Document { x: double; y: double; name: string; columns: Columns; }
//
//     table Pair { first: float; second: float; }
//     table Item { value: float; pair: Pair; }
//     table List { items: [Item]; }
//
//     struct Triple { value: float; first: float; second: float; }
//     table Triples { items: [Triple]; }
//
//     table Sample { key: uint; a: float; b: float; }
//
//     table Strings { items: [string]; }
//
// flatc writes a table's fields from the largest to the smallest, and those
// of one size in reverse order. Its `root_as_*()` functions, which it
// generates for a schema's `root_type`, are `flatbuffers::root()`.

use flatbuffers::{
    Allocator, EndianScalar, FlatBufferBuilder, Follow, ForwardsUOffset, InvalidFlatbuffer, Push,
    PushAlignment, SimpleToVerifyInSlice, Table, TableUnfinishedWIPOffset, VOffsetT, Vector,
    Verifiable, Verifier, WIPOffset, follow_cast_ref,
};

#[derive(Copy, Clone, PartialEq)]
pub struct Record<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Record<'a> {
    type Inner = Record<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Record<'a> {
    pub const VT_X: VOffsetT = 4;
    pub const VT_Y: VOffsetT = 6;
    pub const VT_ID: VOffsetT = 8;
    pub const VT_NAME: VOffsetT = 10;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args RecordArgs<'args>,
    ) -> WIPOffset<Record<'bldr>> {
        let mut builder = RecordBuilder::new(_fbb);
        builder.add_id(args.id);
        builder.add_y(args.y);
        builder.add_x(args.x);
        if let Some(x) = args.name {
            builder.add_name(x);
        }
        builder.finish()
    }

    #[inline]
    pub fn x(&self) -> f64 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f64>(Record::VT_X, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn y(&self) -> f64 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f64>(Record::VT_Y, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn id(&self) -> i64 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<i64>(Record::VT_ID, Some(0)).unwrap() }
    }
    #[inline]
    pub fn name(&self) -> Option<&'a str> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<&str>>(Record::VT_NAME, None)
        }
    }
}

impl Verifiable for Record<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<f64>("x", Self::VT_X, false)?
            .visit_field::<f64>("y", Self::VT_Y, false)?
            .visit_field::<i64>("id", Self::VT_ID, false)?
            .visit_field::<ForwardsUOffset<&str>>("name", Self::VT_NAME, false)?
            .finish();
        Ok(())
    }
}

pub struct RecordArgs<'a> {
    pub x: f64,
    pub y: f64,
    pub id: i64,
    pub name: Option<WIPOffset<&'a str>>,
}

pub struct RecordBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> RecordBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_x(&mut self, x: f64) {
        self.fbb_.push_slot::<f64>(Record::VT_X, x, 0.0);
    }
    #[inline]
    pub fn add_y(&mut self, y: f64) {
        self.fbb_.push_slot::<f64>(Record::VT_Y, y, 0.0);
    }
    #[inline]
    pub fn add_id(&mut self, id: i64) {
        self.fbb_.push_slot::<i64>(Record::VT_ID, id, 0);
    }
    #[inline]
    pub fn add_name(&mut self, name: WIPOffset<&'b str>) {
        self.fbb_
            .push_slot_always::<WIPOffset<_>>(Record::VT_NAME, name);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> RecordBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        RecordBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Record<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Columns<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Columns<'a> {
    type Inner = Columns<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Columns<'a> {
    pub const VT_BASE: VOffsetT = 4;
    pub const VT_KEYS: VOffsetT = 6;
    pub const VT_A: VOffsetT = 8;
    pub const VT_B: VOffsetT = 10;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args ColumnsArgs<'args>,
    ) -> WIPOffset<Columns<'bldr>> {
        let mut builder = ColumnsBuilder::new(_fbb);
        builder.add_base(args.base);
        if let Some(x) = args.b {
            builder.add_b(x);
        }
        if let Some(x) = args.a {
            builder.add_a(x);
        }
        if let Some(x) = args.keys {
            builder.add_keys(x);
        }
        builder.finish()
    }

    #[inline]
    pub fn base(&self) -> i64 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<i64>(Columns::VT_BASE, Some(0)).unwrap() }
    }
    #[inline]
    pub fn keys(&self) -> Option<Vector<'a, u32>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Vector<'a, u32>>>(Columns::VT_KEYS, None)
        }
    }
    #[inline]
    pub fn a(&self) -> Option<Vector<'a, f32>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Vector<'a, f32>>>(Columns::VT_A, None)
        }
    }
    #[inline]
    pub fn b(&self) -> Option<Vector<'a, f32>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Vector<'a, f32>>>(Columns::VT_B, None)
        }
    }
}

impl Verifiable for Columns<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<i64>("base", Self::VT_BASE, false)?
            .visit_field::<ForwardsUOffset<Vector<'_, u32>>>("keys", Self::VT_KEYS, false)?
            .visit_field::<ForwardsUOffset<Vector<'_, f32>>>("a", Self::VT_A, false)?
            .visit_field::<ForwardsUOffset<Vector<'_, f32>>>("b", Self::VT_B, false)?
            .finish();
        Ok(())
    }
}

pub struct ColumnsArgs<'a> {
    pub base: i64,
    pub keys: Option<WIPOffset<Vector<'a, u32>>>,
    pub a: Option<WIPOffset<Vector<'a, f32>>>,
    pub b: Option<WIPOffset<Vector<'a, f32>>>,
}

pub struct ColumnsBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> ColumnsBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_base(&mut self, base: i64) {
        self.fbb_.push_slot::<i64>(Columns::VT_BASE, base, 0);
    }
    #[inline]
    pub fn add_keys(&mut self, keys: WIPOffset<Vector<'b, u32>>) {
        self.fbb_
            .push_slot_always::<WIPOffset<_>>(Columns::VT_KEYS, keys);
    }
    #[inline]
    pub fn add_a(&mut self, a: WIPOffset<Vector<'b, f32>>) {
        self.fbb_.push_slot_always::<WIPOffset<_>>(Columns::VT_A, a);
    }
    #[inline]
    pub fn add_b(&mut self, b: WIPOffset<Vector<'b, f32>>) {
        self.fbb_.push_slot_always::<WIPOffset<_>>(Columns::VT_B, b);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> ColumnsBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        ColumnsBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Columns<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Document<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Document<'a> {
    type Inner = Document<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Document<'a> {
    pub const VT_X: VOffsetT = 4;
    pub const VT_Y: VOffsetT = 6;
    pub const VT_NAME: VOffsetT = 8;
    pub const VT_COLUMNS: VOffsetT = 10;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args DocumentArgs<'args>,
    ) -> WIPOffset<Document<'bldr>> {
        let mut builder = DocumentBuilder::new(_fbb);
        builder.add_y(args.y);
        builder.add_x(args.x);
        if let Some(x) = args.columns {
            builder.add_columns(x);
        }
        if let Some(x) = args.name {
            builder.add_name(x);
        }
        builder.finish()
    }

    #[inline]
    pub fn x(&self) -> f64 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f64>(Document::VT_X, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn y(&self) -> f64 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f64>(Document::VT_Y, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn name(&self) -> Option<&'a str> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<&str>>(Document::VT_NAME, None)
        }
    }
    #[inline]
    pub fn columns(&self) -> Option<Columns<'a>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Columns>>(Document::VT_COLUMNS, None)
        }
    }
}

impl Verifiable for Document<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<f64>("x", Self::VT_X, false)?
            .visit_field::<f64>("y", Self::VT_Y, false)?
            .visit_field::<ForwardsUOffset<&str>>("name", Self::VT_NAME, false)?
            .visit_field::<ForwardsUOffset<Columns>>("columns", Self::VT_COLUMNS, false)?
            .finish();
        Ok(())
    }
}

pub struct DocumentArgs<'a> {
    pub x: f64,
    pub y: f64,
    pub name: Option<WIPOffset<&'a str>>,
    pub columns: Option<WIPOffset<Columns<'a>>>,
}

pub struct DocumentBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> DocumentBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_x(&mut self, x: f64) {
        self.fbb_.push_slot::<f64>(Document::VT_X, x, 0.0);
    }
    #[inline]
    pub fn add_y(&mut self, y: f64) {
        self.fbb_.push_slot::<f64>(Document::VT_Y, y, 0.0);
    }
    #[inline]
    pub fn add_name(&mut self, name: WIPOffset<&'b str>) {
        self.fbb_
            .push_slot_always::<WIPOffset<_>>(Document::VT_NAME, name);
    }
    #[inline]
    pub fn add_columns(&mut self, columns: WIPOffset<Columns<'b>>) {
        self.fbb_
            .push_slot_always::<WIPOffset<Columns>>(Document::VT_COLUMNS, columns);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> DocumentBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        DocumentBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Document<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Pair<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Pair<'a> {
    type Inner = Pair<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Pair<'a> {
    pub const VT_FIRST: VOffsetT = 4;
    pub const VT_SECOND: VOffsetT = 6;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args PairArgs,
    ) -> WIPOffset<Pair<'bldr>> {
        let mut builder = PairBuilder::new(_fbb);
        builder.add_second(args.second);
        builder.add_first(args.first);
        builder.finish()
    }

    #[inline]
    pub fn first(&self) -> f32 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f32>(Pair::VT_FIRST, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn second(&self) -> f32 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f32>(Pair::VT_SECOND, Some(0.0)).unwrap() }
    }
}

impl Verifiable for Pair<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<f32>("first", Self::VT_FIRST, false)?
            .visit_field::<f32>("second", Self::VT_SECOND, false)?
            .finish();
        Ok(())
    }
}

pub struct PairArgs {
    pub first: f32,
    pub second: f32,
}

pub struct PairBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> PairBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_first(&mut self, first: f32) {
        self.fbb_.push_slot::<f32>(Pair::VT_FIRST, first, 0.0);
    }
    #[inline]
    pub fn add_second(&mut self, second: f32) {
        self.fbb_.push_slot::<f32>(Pair::VT_SECOND, second, 0.0);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> PairBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        PairBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Pair<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Item<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Item<'a> {
    type Inner = Item<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Item<'a> {
    pub const VT_VALUE: VOffsetT = 4;
    pub const VT_PAIR: VOffsetT = 6;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args ItemArgs<'args>,
    ) -> WIPOffset<Item<'bldr>> {
        let mut builder = ItemBuilder::new(_fbb);
        if let Some(x) = args.pair {
            builder.add_pair(x);
        }
        builder.add_value(args.value);
        builder.finish()
    }

    #[inline]
    pub fn value(&self) -> f32 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f32>(Item::VT_VALUE, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn pair(&self) -> Option<Pair<'a>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<ForwardsUOffset<Pair>>(Item::VT_PAIR, None) }
    }
}

impl Verifiable for Item<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<f32>("value", Self::VT_VALUE, false)?
            .visit_field::<ForwardsUOffset<Pair>>("pair", Self::VT_PAIR, false)?
            .finish();
        Ok(())
    }
}

pub struct ItemArgs<'a> {
    pub value: f32,
    pub pair: Option<WIPOffset<Pair<'a>>>,
}

pub struct ItemBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> ItemBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_value(&mut self, value: f32) {
        self.fbb_.push_slot::<f32>(Item::VT_VALUE, value, 0.0);
    }
    #[inline]
    pub fn add_pair(&mut self, pair: WIPOffset<Pair<'b>>) {
        self.fbb_
            .push_slot_always::<WIPOffset<Pair>>(Item::VT_PAIR, pair);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> ItemBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        ItemBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Item<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct List<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for List<'a> {
    type Inner = List<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> List<'a> {
    pub const VT_ITEMS: VOffsetT = 4;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args ListArgs<'args>,
    ) -> WIPOffset<List<'bldr>> {
        let mut builder = ListBuilder::new(_fbb);
        if let Some(x) = args.items {
            builder.add_items(x);
        }
        builder.finish()
    }

    #[inline]
    pub fn items(&self) -> Option<Vector<'a, ForwardsUOffset<Item<'a>>>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Vector<'a, ForwardsUOffset<Item>>>>(List::VT_ITEMS, None)
        }
    }
}

impl Verifiable for List<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<ForwardsUOffset<Vector<'_, ForwardsUOffset<Item>>>>(
                "items",
                Self::VT_ITEMS,
                false,
            )?
            .finish();
        Ok(())
    }
}

pub struct ListArgs<'a> {
    pub items: Option<WIPOffset<Vector<'a, ForwardsUOffset<Item<'a>>>>>,
}

pub struct ListBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> ListBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_items(&mut self, items: WIPOffset<Vector<'b, ForwardsUOffset<Item<'b>>>>) {
        self.fbb_
            .push_slot_always::<WIPOffset<_>>(List::VT_ITEMS, items);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> ListBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        ListBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<List<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

// struct Triple, aligned to 4
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq)]
pub struct Triple(pub [u8; 12]);

impl Default for Triple {
    fn default() -> Self {
        Self([0; 12])
    }
}

impl SimpleToVerifyInSlice for Triple {}

impl<'a> Follow<'a> for Triple {
    type Inner = &'a Triple;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        unsafe { <&'a Triple>::follow(buf, loc) }
    }
}

impl<'a> Follow<'a> for &'a Triple {
    type Inner = &'a Triple;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        unsafe { follow_cast_ref::<Triple>(buf, loc) }
    }
}

impl<'b> Push for Triple {
    type Output = Triple;
    #[inline]
    unsafe fn push(&self, dst: &mut [u8], _written_len: usize) {
        let src = unsafe {
            ::core::slice::from_raw_parts(
                self as *const Triple as *const u8,
                <Self as Push>::size(),
            )
        };
        dst.copy_from_slice(src);
    }
    #[inline]
    fn alignment() -> PushAlignment {
        PushAlignment::new(4)
    }
}

impl<'a> Verifiable for Triple {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.in_buffer::<Self>(pos)
    }
}

impl<'a> Triple {
    #[allow(clippy::too_many_arguments)]
    pub fn new(value: f32, first: f32, second: f32) -> Self {
        let mut s = Self([0; 12]);
        s.set_value(value);
        s.set_first(first);
        s.set_second(second);
        s
    }

    pub fn value(&self) -> f32 {
        let mut mem = core::mem::MaybeUninit::<<f32 as EndianScalar>::Scalar>::uninit();
        // Safety:
        // Created from a valid Table for this object
        // Which contains a valid value in this slot
        EndianScalar::from_little_endian(unsafe {
            core::ptr::copy_nonoverlapping(
                self.0[0..].as_ptr(),
                mem.as_mut_ptr() as *mut u8,
                core::mem::size_of::<<f32 as EndianScalar>::Scalar>(),
            );
            mem.assume_init()
        })
    }

    pub fn set_value(&mut self, x: f32) {
        let x_le = x.to_little_endian();
        // Safety:
        // Created from a valid Table for this object
        // Which contains a valid value in this slot
        unsafe {
            core::ptr::copy_nonoverlapping(
                &x_le as *const _ as *const u8,
                self.0[0..].as_mut_ptr(),
                core::mem::size_of::<<f32 as EndianScalar>::Scalar>(),
            );
        }
    }

    pub fn first(&self) -> f32 {
        let mut mem = core::mem::MaybeUninit::<<f32 as EndianScalar>::Scalar>::uninit();
        // Safety:
        // Created from a valid Table for this object
        // Which contains a valid value in this slot
        EndianScalar::from_little_endian(unsafe {
            core::ptr::copy_nonoverlapping(
                self.0[4..].as_ptr(),
                mem.as_mut_ptr() as *mut u8,
                core::mem::size_of::<<f32 as EndianScalar>::Scalar>(),
            );
            mem.assume_init()
        })
    }

    pub fn set_first(&mut self, x: f32) {
        let x_le = x.to_little_endian();
        // Safety:
        // Created from a valid Table for this object
        // Which contains a valid value in this slot
        unsafe {
            core::ptr::copy_nonoverlapping(
                &x_le as *const _ as *const u8,
                self.0[4..].as_mut_ptr(),
                core::mem::size_of::<<f32 as EndianScalar>::Scalar>(),
            );
        }
    }

    pub fn second(&self) -> f32 {
        let mut mem = core::mem::MaybeUninit::<<f32 as EndianScalar>::Scalar>::uninit();
        // Safety:
        // Created from a valid Table for this object
        // Which contains a valid value in this slot
        EndianScalar::from_little_endian(unsafe {
            core::ptr::copy_nonoverlapping(
                self.0[8..].as_ptr(),
                mem.as_mut_ptr() as *mut u8,
                core::mem::size_of::<<f32 as EndianScalar>::Scalar>(),
            );
            mem.assume_init()
        })
    }

    pub fn set_second(&mut self, x: f32) {
        let x_le = x.to_little_endian();
        // Safety:
        // Created from a valid Table for this object
        // Which contains a valid value in this slot
        unsafe {
            core::ptr::copy_nonoverlapping(
                &x_le as *const _ as *const u8,
                self.0[8..].as_mut_ptr(),
                core::mem::size_of::<<f32 as EndianScalar>::Scalar>(),
            );
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Triples<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Triples<'a> {
    type Inner = Triples<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Triples<'a> {
    pub const VT_ITEMS: VOffsetT = 4;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args TriplesArgs<'args>,
    ) -> WIPOffset<Triples<'bldr>> {
        let mut builder = TriplesBuilder::new(_fbb);
        if let Some(x) = args.items {
            builder.add_items(x);
        }
        builder.finish()
    }

    #[inline]
    pub fn items(&self) -> Option<Vector<'a, Triple>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Vector<'a, Triple>>>(Triples::VT_ITEMS, None)
        }
    }
}

impl Verifiable for Triples<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<ForwardsUOffset<Vector<'_, Triple>>>("items", Self::VT_ITEMS, false)?
            .finish();
        Ok(())
    }
}

pub struct TriplesArgs<'a> {
    pub items: Option<WIPOffset<Vector<'a, Triple>>>,
}

pub struct TriplesBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> TriplesBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_items(&mut self, items: WIPOffset<Vector<'b, Triple>>) {
        self.fbb_
            .push_slot_always::<WIPOffset<_>>(Triples::VT_ITEMS, items);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> TriplesBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        TriplesBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Triples<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Sample<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Sample<'a> {
    type Inner = Sample<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Sample<'a> {
    pub const VT_KEY: VOffsetT = 4;
    pub const VT_A: VOffsetT = 6;
    pub const VT_B: VOffsetT = 8;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args SampleArgs,
    ) -> WIPOffset<Sample<'bldr>> {
        let mut builder = SampleBuilder::new(_fbb);
        builder.add_b(args.b);
        builder.add_a(args.a);
        builder.add_key(args.key);
        builder.finish()
    }

    #[inline]
    pub fn key(&self) -> u32 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<u32>(Sample::VT_KEY, Some(0)).unwrap() }
    }
    #[inline]
    pub fn a(&self) -> f32 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f32>(Sample::VT_A, Some(0.0)).unwrap() }
    }
    #[inline]
    pub fn b(&self) -> f32 {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe { self._tab.get::<f32>(Sample::VT_B, Some(0.0)).unwrap() }
    }
}

impl Verifiable for Sample<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<u32>("key", Self::VT_KEY, false)?
            .visit_field::<f32>("a", Self::VT_A, false)?
            .visit_field::<f32>("b", Self::VT_B, false)?
            .finish();
        Ok(())
    }
}

pub struct SampleArgs {
    pub key: u32,
    pub a: f32,
    pub b: f32,
}

pub struct SampleBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> SampleBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_key(&mut self, key: u32) {
        self.fbb_.push_slot::<u32>(Sample::VT_KEY, key, 0);
    }
    #[inline]
    pub fn add_a(&mut self, a: f32) {
        self.fbb_.push_slot::<f32>(Sample::VT_A, a, 0.0);
    }
    #[inline]
    pub fn add_b(&mut self, b: f32) {
        self.fbb_.push_slot::<f32>(Sample::VT_B, b, 0.0);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> SampleBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        SampleBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Sample<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Strings<'a> {
    pub _tab: Table<'a>,
}

impl<'a> Follow<'a> for Strings<'a> {
    type Inner = Strings<'a>;
    #[inline]
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        Self {
            _tab: unsafe { Table::new(buf, loc) },
        }
    }
}

impl<'a> Strings<'a> {
    pub const VT_ITEMS: VOffsetT = 4;

    #[allow(unused_mut)]
    pub fn create<'bldr: 'args, 'args: 'mut_bldr, 'mut_bldr, A: Allocator + 'bldr>(
        _fbb: &'mut_bldr mut FlatBufferBuilder<'bldr, A>,
        args: &'args StringsArgs<'args>,
    ) -> WIPOffset<Strings<'bldr>> {
        let mut builder = StringsBuilder::new(_fbb);
        if let Some(x) = args.items {
            builder.add_items(x);
        }
        builder.finish()
    }

    #[inline]
    pub fn items(&self) -> Option<Vector<'a, ForwardsUOffset<&'a str>>> {
        // Safety:
        // Created from valid Table for this object
        // which contains a valid value in this slot
        unsafe {
            self._tab
                .get::<ForwardsUOffset<Vector<'a, ForwardsUOffset<&'a str>>>>(
                    Strings::VT_ITEMS,
                    None,
                )
        }
    }
}

impl Verifiable for Strings<'_> {
    #[inline]
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<ForwardsUOffset<Vector<'_, ForwardsUOffset<&'_ str>>>>(
                "items",
                Self::VT_ITEMS,
                false,
            )?
            .finish();
        Ok(())
    }
}

pub struct StringsArgs<'a> {
    pub items: Option<WIPOffset<Vector<'a, ForwardsUOffset<&'a str>>>>,
}

pub struct StringsBuilder<'a: 'b, 'b, A: Allocator + 'a> {
    fbb_: &'b mut FlatBufferBuilder<'a, A>,
    start_: WIPOffset<TableUnfinishedWIPOffset>,
}

impl<'a: 'b, 'b, A: Allocator + 'a> StringsBuilder<'a, 'b, A> {
    #[inline]
    pub fn add_items(&mut self, items: WIPOffset<Vector<'b, ForwardsUOffset<&'b str>>>) {
        self.fbb_
            .push_slot_always::<WIPOffset<_>>(Strings::VT_ITEMS, items);
    }
    #[inline]
    pub fn new(_fbb: &'b mut FlatBufferBuilder<'a, A>) -> StringsBuilder<'a, 'b, A> {
        let start = _fbb.start_table();
        StringsBuilder {
            fbb_: _fbb,
            start_: start,
        }
    }
    #[inline]
    pub fn finish(self) -> WIPOffset<Strings<'a>> {
        let o = self.fbb_.end_table(self.start_);
        WIPOffset::new(o.value())
    }
}
