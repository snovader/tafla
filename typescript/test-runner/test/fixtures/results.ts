// Runs a file's tests with node:test, and prints their results as JSON, for
// `declarations.test.ts`, whose own run can't run other files:
//
//     node results.ts <file> [only]
import process from 'node:process';
import { run } from 'node:test';
import type { TestEvent } from 'node:test/reporters';
import { inspect } from 'node:util';

const [file, only] = process.argv.slice(2);
const results: unknown[] = [];
const events: AsyncIterable<TestEvent> = run({
    files: [file],
    isolation: 'none',
    only: only === 'only'
});

for await (const event of events) {
    if (event.type === 'test:pass' || event.type === 'test:fail') {
        const { data } = event;
        results.push({
            name: data.name,
            passed: event.type === 'test:pass',
            skipped: data.skip !== undefined && data.skip !== false,
            todo: data.todo !== undefined && data.todo !== false,
            file: data.file,
            line: data.line,
            error:
                event.type === 'test:fail'
                    ? inspect(event.data.details.error)
                    : ''
        });
    }
}

process.stdout.write(JSON.stringify(results));
