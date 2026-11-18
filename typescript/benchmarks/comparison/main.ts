import { flatbuffers } from './codecs/flatbuffers.ts';
import { msgpackr } from './codecs/msgpackr.ts';
import { protobufjs } from './codecs/protobuf.ts';
import { tafla } from './codecs/tafla.ts';
import { taflaGen } from './codecs/taflaGen.ts';
import { describeRun, runOptions } from './harness.ts';
import { getterAccess, tableReaderAccess } from './itemAccess.ts';
import { runSuite } from './suite.ts';

const options = runOptions(process.argv.slice(2));

console.log(describeRun(options));

// The getters twice, an A/A pair that shows the harness's noise
const accesses = [
    tableReaderAccess,
    getterAccess,
    { ...getterAccess, name: 'tafla-gen A/A' }
];

runSuite([tafla, taflaGen, flatbuffers, protobufjs, msgpackr], accesses, {
    ...options,
    print: line => console.log(line)
});
