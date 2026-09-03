import { describe, expect, it } from 'vitest';
import { createTask } from '../src/domain/create-task';
import { postTask } from '../src/api/task-controller';

describe('task creation', () => {
  it('creates tasks through the domain function and its indirect API caller', () => {
    expect(createTask({ title: 'Direct' }).title).toBe('Direct');
    expect(postTask('Indirect').title).toBe('Indirect');
  });
});
