// Bundles the package once per run of `vitest run`, before any project's
// workers import it, and removes the bundle after the run. The root config's
// global setup runs once, rather than once per project.
import type { TestProject } from 'vitest/node';
import { bundle, removeBundle } from './bundle.ts';

export async function setup(project: TestProject): Promise<void> {
    await bundle();

    // Watch mode reruns tests after a change to the source, which the bundle
    // has to include
    project.onTestsRerun(async () => {
        await bundle();
    });
}

export async function teardown(): Promise<void> {
    await removeBundle();
}
