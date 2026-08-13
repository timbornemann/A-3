import type { AgentApprovalResponseV1 } from './agent-approval';

const id = (value: string): string => value.repeat(64);

export function patchApprovalResponse(
  status: 'pending' | 'active' = 'pending',
): AgentApprovalResponseV1 {
  return {
    protocolVersion: 1,
    result: {
      approval: {
        action: {
          kind: 'patch',
          patch: {
            files: [
              {
                operation: 'update',
                sourcePath: { displayPath: 'src/lib.rs', pathHex: '7372632f6c69622e7273' },
                targetPath: { displayPath: 'src/lib.rs', pathHex: '7372632f6c69622e7273' },
              },
            ],
            rationale: 'Den eng begrenzten Fehler beheben.',
          },
        },
        actionClass: 'write',
        approvalRevision: status === 'pending' ? '3' : '4',
        canAllowOnce: status === 'pending',
        canContinue: status === 'active',
        canDeny: status === 'pending',
        canRevoke: status === 'active',
        controllerState: 'awaitApproval',
        expiresAtUnixMillis: '2000',
        ledgerRevision: 2,
        ledgerStoreVersion: '7',
        reason: 'systemPolicy',
        requestedAtUnixMillis: '1000',
        risk: 'moderate',
        scopeDigest: id('a'),
        snapshotId: id('5'),
        status,
        stepId: id('3'),
        stepStatus: 'awaitingApproval',
      },
      status: 'available',
    },
  };
}
