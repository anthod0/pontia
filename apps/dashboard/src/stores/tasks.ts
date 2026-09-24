import { get, writable } from 'svelte/store';
import {
  cancelTask as apiCancelTask,
  getTask,
  interruptTask as apiInterruptTask,
  listTaskEvents,
  listTasks,
} from '../api/client';
import type {
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

let listRequest = 0;
let detailRequest = 0;

export async function loadTasks(): Promise<void> {
  const request = ++listRequest;
  tasksLoading.set(true);
  tasksError.set(null);
  try {
    const loaded = await listTasks();
    if (request === listRequest) tasks.set(loaded);
  } catch (error) {
    if (request === listRequest) tasksError.set(error instanceof Error ? error.message : String(error));
  } finally {
    if (request === listRequest) tasksLoading.set(false);
  }
}

export async function selectTask(taskId: string): Promise<void> {
  selectedTaskId.set(taskId);
  await refreshTask(taskId);
}

export async function refreshTask(taskId: string): Promise<void> {
  const request = ++detailRequest;
  const isCurrent = () => request === detailRequest && (!get(selectedTaskId) || get(selectedTaskId) === taskId);
  taskLoading.set(true);
  taskError.set(null);
  try {
    const [taskView, events] = await Promise.all([getTask(taskId), listTaskEvents(taskId)]);
    if (!isCurrent()) return;
    task.set(taskView);
    taskEvents.set(events);
  } catch (error) {
    if (isCurrent()) taskError.set(error instanceof Error ? error.message : String(error));
  } finally {
    if (isCurrent()) taskLoading.set(false);
  }
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
