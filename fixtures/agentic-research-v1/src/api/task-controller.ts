import { scheduleTask } from '../services/task-service';

export function postTask(title: string) {
  return scheduleTask({ title });
}
