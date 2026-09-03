import { scheduleTask } from '../services/task-service';

export function backfillMissingTask(name: string) {
  return scheduleTask({ title: name });
}
