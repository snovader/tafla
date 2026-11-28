#!/usr/bin/env node
import { runCommand } from './command.ts';

const output = runCommand(process.argv.slice(2));

process.stdout.write(output.stdout);
process.stderr.write(output.stderr);
process.exitCode = output.code;
