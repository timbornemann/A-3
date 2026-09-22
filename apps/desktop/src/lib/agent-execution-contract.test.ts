import { describe, expect, it } from 'vitest';
import projections from './agent-execution-contract.fixture.json';
import { parseAgentApprovalResponseV1 } from './agent-approval';
import { parseAgentInspectionResponseV1 } from './agent-inspection';
import { parseAgentActivityResponseV1 } from './agent-activity';
import { parseTaskLensTaskResponseV1 } from './task-lens';

// Generated from the real offline Rust executor, libSQL store, patch application,
// command discovery and Python unittest run. See Plan 19 for regeneration.
describe('Core execution projections at the frontend boundary', () => {
  it('covers patch approval, process approval and the final verification', () => {
    expect(projections.filter((projection) => projection.kind === 'approval')).toHaveLength(2);
    expect(projections.filter((projection) => projection.kind === 'inspection')).toHaveLength(3);
    expect(projections.filter((projection) => projection.kind === 'activity')).toHaveLength(3);
    expect(projections.filter((projection) => projection.kind === 'workPlan')).toHaveLength(3);
  });

  for (const [index, projection] of projections.entries()) {
    it(`decodes the real ${projection.kind} response at stage ${index}`, () => {
      const decoded =
        projection.kind === 'workPlan'
          ? parseTaskLensTaskResponseV1(projection.response)
          : projection.kind === 'activity'
            ? parseAgentActivityResponseV1(projection.response)
            : projection.kind === 'approval'
              ? parseAgentApprovalResponseV1(projection.response)
              : parseAgentInspectionResponseV1(projection.response);
      expect(decoded.result.status).toBe('available');
    });
  }
});
