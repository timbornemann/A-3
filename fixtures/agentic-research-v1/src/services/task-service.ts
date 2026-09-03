import { createTask as persistTask, type NewTask, type StoredTask } from '../domain/create-task';

export function scheduleTask(input: NewTask): StoredTask {
  return persistTask(input);
}
