import type { RouterConf } from 'svelte-mini-router'

export const routerConf: RouterConf = {
  baseUrl: '/dashboard',
  routes: [
    { path: '/', render: () => import('./pages/OverviewPage.svelte') },
    { path: '/overview', render: () => import('./pages/OverviewPage.svelte') },
    { path: '/tasks', render: () => import('./pages/TasksPage.svelte') },
    { path: '/tasks/{taskId}/overview', render: () => import('./pages/task/TaskOverviewPage.svelte') },
    { path: '/tasks/{taskId}/dag', render: () => import('./pages/task/TaskDagPage.svelte') },
    { path: '/tasks/{taskId}/work-items', render: () => import('./pages/task/TaskWorkItemsPage.svelte') },
    { path: '/tasks/{taskId}/sessions', render: () => import('./pages/task/TaskSessionsPage.svelte') },
    { path: '/tasks/{taskId}/artifacts', render: () => import('./pages/task/TaskArtifactsPage.svelte') },
    { path: '/tasks/{taskId}/activity', render: () => import('./pages/task/TaskActivityPage.svelte') },
    { path: '/workspaces', render: () => import('./pages/WorkspacesPage.svelte') },
    { path: '/agent-profiles', render: () => import('./pages/AgentProfilesPage.svelte') },
    { path: '/settings', render: () => import('./pages/SettingsPage.svelte') },
  ],
  render404: () => import('./pages/NotFoundPage.svelte'),
}
