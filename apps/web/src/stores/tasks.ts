import { writable } from 'svelte/store';
import {
  cancelTask as apiCancelTask,
  confirmTaskWorkspace as apiConfirmTaskWorkspace,
  createTask as apiCreateTask,
  getTask,
  interruptTask as apiInterruptTask,
  listTaskEvents,
  listTasks,
  submitPlannerInput as apiSubmitPlannerInput,
} from '../api/client';
import type {
  ConfirmTaskWorkspaceInput,
  CreateTaskInput,
  SubmitPlannerInput,
  TaskEventView,
  TaskView,
} from '../api/types';

export const tasks = writable<TaskView[]>([]);
export const tasksLoading = writable(false);
export const tasksError = writable<string | null>(null);

export const selectedTaskId = writable<string | null>(null);
export const task = writable<TaskView | null>(null);
export const taskEvents = writable<TaskEventView[]>([]);
export const taskLoading = writable(false);
export const taskError = writable<string | null>(null);

export async function loadTasks(): Promise<void> {
  tasksLoading.set(true);
  tasksError.set(null);
  try {
    tasks.set(await listTasks());
  } catch (error) {
    tasksError.set(error instanceof Error ? error.message : String(error));
  } finally {
    tasksLoading.set(false);
  }
}

export async function selectTask(taskId: string): Promise<void> {
  selectedTaskId.set(taskId);
  await refreshTask(taskId);
}

export async function refreshTask(taskId: string): Promise<void> {
  taskLoading.set(true);
  taskError.set(null);
  try {
    const [taskView, events] = await Promise.all([getTask(taskId), listTaskEvents(taskId)]);
    task.set(taskView);
    taskEvents.set(events);
  } catch (error) {
    taskError.set(error instanceof Error ? error.message : String(error));
  } finally {
    taskLoading.set(false);
  }
}

export async function createTask(input: CreateTaskInput): Promise<TaskView> {
  const created = await apiCreateTask(input);
  await loadTasks();
  selectedTaskId.set(created.task_id);
  task.set(created);
  taskEvents.set(await listTaskEvents(created.task_id));
  return created;
}

export async function confirmTaskWorkspace(taskId: string, input: ConfirmTaskWorkspaceInput): Promise<TaskView> {
  const updated = await apiConfirmTaskWorkspace(taskId, input);
  await Promise.all([loadTasks(), refreshTask(taskId)]);
  return updated;
}

export async function submitPlannerInput(taskId: string, input: SubmitPlannerInput): Promise<TaskView> {
  const updated = await apiSubmitPlannerInput(taskId, input);
  await Promise.all([loadTasks(), refreshTask(taskId)]);
  return updated;
}

export async function cancelTask(taskId: string): Promise<TaskView> {
  const updated = await apiCancelTask(taskId);
  await Promise.all([loadTasks(), refreshTask(taskId)]);
  return updated;
}

export async function interruptTask(taskId: string): Promise<TaskView> {
  const updated = await apiInterruptTask(taskId);
  await Promise.all([loadTasks(), refreshTask(taskId)]);
  return updated;
}
