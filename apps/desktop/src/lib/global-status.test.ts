import { describe, expect, it } from 'vitest';
import { workspaceAreaFromHash } from './global-status';

describe('workspaceAreaFromHash', () => {
  it.each([
    ['#projects', 'projects'],
    ['#map', 'map'],
    ['#agent', 'agent'],
    ['#settings', 'settings'],
  ] as const)('restores the %s route', (hash, expected) => {
    expect(workspaceAreaFromHash(hash)).toBe(expected);
  });

  it('falls back to Projects for an unknown or empty route', () => {
    expect(workspaceAreaFromHash('#untrusted')).toBe('projects');
    expect(workspaceAreaFromHash('')).toBe('projects');
  });
});
