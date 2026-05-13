import { writable } from 'svelte/store';
import {
  cancelTask as apiCancelTask,
  confirmTaskWorkspace as apiConfirmTaskWorkspace,
  createDagTask as apiCreateDagTask,
  createHumanSignal as apiCreateHumanSignal,
  createTask as apiCreateTask,
  getTask,
  getTaskDag,
  interruptTask as apiInterruptTask,
  listTaskEvents,
  pauseTask as apiPauseTask,
  listTasks,
  resumeTask as apiResumeTask,
  submitPlannerInput as apiSubmitPlannerInput,
} from '../api/client';
import type {
  ConfirmTaskWorkspaceInput,
  CreateDagTaskResult,
  CreateTaskInput,
  HumanSignalInput,
  SubmitPlannerInput,
  TaskDagView,
  TaskEventView,
  TaskView,
} from '../api/types';

export const tasks = writable<TaskView[]>([]);
export const tasksLoading = writable(false);
export const tasksError = writable<string | null>(null);

export const selectedTaskId = writable<string | null>(null);
export const task = writable<TaskView | null>(null);
export const taskEvents = writable<TaskEventView[]>([]);
export const taskDag = writable<TaskDagView | null>(null);
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
    const [taskView, events, dag] = await Promise.all([getTask(taskId), listTaskEvents(taskId), getTaskDag(taskId)]);
    task.set(taskView);
    taskEvents.set(events);
    taskDag.set(dag);
  } catch (error) {
    taskDag.set(null);
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
  const [events, dag] = await Promise.all([listTaskEvents(created.task_id), getTaskDag(created.task_id)]);
  taskEvents.set(events);
  taskDag.set(dag);
  return created;
}

export async function createDagTask(input: CreateTaskInput): Promise<CreateDagTaskResult> {
  const result = await apiCreateDagTask(input);
  await loadTasks();
  selectedTaskId.set(result.task.task_id);
  task.set(result.task);
  const [events, dag] = await Promise.all([listTaskEvents(result.task.task_id), getTaskDag(result.task.task_id)]);
  taskEvents.set(events);
  taskDag.set(dag);
  return result;
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

export async function pauseTask(taskId: string): Promise<TaskView> {
  const updated = await apiPauseTask(taskId);
  await Promise.all([loadTasks(), refreshTask(taskId)]);
  return updated;
}

export async function resumeTask(taskId: string): Promise<TaskView> {
  const result = await apiResumeTask(taskId);
  await Promise.all([loadTasks(), refreshTask(taskId)]);
  return result.task;
}

export async function createHumanSignal(taskId: string, input: HumanSignalInput): Promise<void> {
  await apiCreateHumanSignal(taskId, input);
  await refreshTask(taskId);
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
