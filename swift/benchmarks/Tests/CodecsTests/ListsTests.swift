// The lists suite's codecs read back every list, and the generator makes the
// TypeScript suite's strings, which these were printed from, so that the
// languages' lists suites time the same work

import Codecs
import Testing
import Workloads

@Test func taflaGenReadsBackEveryListAsAVectorOfStrings() throws {
    try checkStringLists(TaflaGenStringList())
}

@Test func taflaGenReadsBackEveryListAsAVectorOfTables() throws {
    try checkStringLists(TaflaGenNameList())
}

@Test func taflaGenWritesTheVectorsOfStringsThatTableBuilderWrites() throws {
    try checkStringListsAsTableBuilderWritesThem()
}

/// Each list's name, the first 16 characters of its first and last strings,
/// and the UTF-8 length of all its strings, as the TypeScript suite's
/// `shapes.ts` made them
@Test(
    arguments: [
        ("10 × 16 chars of ASCII", "2FsyeXxp1ewxJijA", "CcfRsv23GwWuZIqm", 160),
        ("10 × 256 chars of ASCII", "2GYBM2rvPSVojjhN", "3VpjDEeuixVZfJJy", 2_560),
        ("10 × 8 chars of 3 bytes", "鑡絞眙蜩榩眳矝愞", "琑翘趠谎衬耂竎刓", 240),
        ("100 × 16 chars of ASCII", "2N45gJLJhyrBp817", "GuM8hMtpdCgpm337", 1_600),
        ("100 × 256 chars of ASCII", "2OC9KSUHzW0k790o", "e1d0Fdjo5UszbCUg", 25_600),
        ("100 × 8 chars of 3 bytes", "鑗蝝姘徃栲鈞甕顽", "鱵髩痧剻熉五憓荜", 2_400),
        ("10,000 × 16 chars of ASCII", "2VGBSOLG6JrrCjwD", "FzBrmc8Y0jcGbWah", 160_000),
        ("10,000 × 256 chars of ASCII", "2WgxnHUMGp0eUivR", "VU0lrgd6V0hWxlMq", 2_560_000),
        ("10,000 × 8 chars of 3 bytes", "鑶鄷攴臚鷑靆畖鱯", "淟坓惗聳侞忝鞂姩", 240_000),
    ] as [(String, String, String, Int)]
)
func generatesTheTypeScriptLists(name: String, first: String, last: String, utf8Count: Int) throws {
    let shape = try #require(listShapes.first { $0.name == name })

    #expect(String(shape.strings[0].unicodeScalars.prefix(16)) == first)
    #expect(String(shape.strings[shape.strings.count - 1].unicodeScalars.prefix(16)) == last)
    #expect(shape.strings.reduce(0) { $0 + $1.utf8.count } == utf8Count)
}

@Test func timesNineLists() {
    #expect(listShapes.count == 9)
}
