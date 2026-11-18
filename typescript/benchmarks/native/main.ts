import {
    describeRun,
    measureGroups,
    runOptions
} from '../comparison/harness.ts';
import { nativeGroups } from './groups.ts';

const options = runOptions(process.argv.slice(2));

console.log(describeRun(options));

measureGroups(nativeGroups(), options, line => console.log(line));
