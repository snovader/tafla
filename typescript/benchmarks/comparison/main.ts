import { flatbuffers } from './codecs/flatbuffers.ts';
import { msgpackr } from './codecs/msgpackr.ts';
import { protobufjs } from './codecs/protobuf.ts';
import { taflaGen } from './codecs/tafla-gen.ts';
import { tafla } from './codecs/tafla.ts';
import { getterAccess, tableReaderAccess } from './item-access.ts';
import { runSuite } from './suite.ts';

await runSuite(
    [tafla, taflaGen, flatbuffers, protobufjs, msgpackr],
    [tableReaderAccess, getterAccess],
    process.argv.slice(2)
);
