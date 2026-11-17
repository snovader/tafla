import {
    describeRun,
    measureGroups,
    runOptions
} from '../comparison/harness.ts';
import { fieldGroups } from './groups.ts';

const options = runOptions(process.argv.slice(2));

console.log(describeRun(options));

measureGroups(fieldGroups(), options, line => console.log(line));
