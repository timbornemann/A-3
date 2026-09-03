export interface NewTask {
  title: string;
}

export interface StoredTask extends NewTask {
  id: string;
}

export function createTask(input: NewTask): StoredTask {
  return { id: `task-${input.title.toLowerCase()}`, ...input };
}
