import { tafla } from './codecs/tafla.ts';
import { taflaGen } from './codecs/taflaGen.ts';
import { flatbuffers } from './codecs/flatbuffers.ts';
import { msgpackr } from './codecs/msgpackr.ts';
import { protobufjs } from './codecs/protobuf.ts';
import { byteBufferAccess, getterAccess } from './itemAccess.ts';
import { runSuite } from './suite.ts';

const args = process.argv.slice(2);
const rounds = Number(option('--rounds') ?? 31);

console.log(
    `Node ${process.version}, V8 ${process.versions.v8}; minimum of ${rounds} interleaved rounds, and bytes allocated on V8's heap per call`
);

// The getters twice, an A/A pair that shows the harness's noise
const accesses = [
    byteBufferAccess,
    getterAccess,
    { ...getterAccess, name: 'tafla-gen A/A' }
];

runSuite([tafla, taflaGen, flatbuffers, protobufjs, msgpackr], accesses, {
    rounds,
    warmupRounds: 10,
    sampleTime: Number(option('--sample-ms') ?? 1),
    filter: new RegExp(option('--filter') ?? ''),
    print: line => console.log(line)
});

function option(name: string): string | undefined {
    const index = args.indexOf(name);
    return index === -1 ? undefined : args[index + 1];
}
