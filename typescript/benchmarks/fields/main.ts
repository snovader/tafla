import { bench } from 'benk';
import { fieldGroups } from './groups.ts';

await bench(fieldGroups());
