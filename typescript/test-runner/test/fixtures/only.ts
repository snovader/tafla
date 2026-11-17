// Tests that `declarations.test.ts` runs with `only`
import { expect, it } from 'test-runner';

it.only('only runs', () => {
    expect(true).toBe(true);
});

it('the others are skipped', () => {
    expect('ran').toBe('skipped');
});
