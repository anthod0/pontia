import type { RouterConf } from 'svelte-mini-router'

export const routerConf: RouterConf = {
  baseUrl: '/dashboard',
  routes: [
    { path: '/', render: () => import('./pages/NewChatPage.svelte') },
    { path: '/workspaces', render: () => import('./pages/WorkspacesPage.svelte') },
    { path: '/workspace/{workspaceId}', render: () => import('./pages/WorkspacePage.svelte') },
    { path: '/chat', render: () => import('./pages/NewChatPage.svelte') },
    { path: '/chat/{sessionId}', render: () => import('./pages/SessionChatPage.svelte') },
    { path: '/sessions', render: () => import('./pages/SessionsPage.svelte') },
    { path: '/sessions/{sessionId}', render: () => import('./pages/SessionDetailPage.svelte') },
    { path: '/agent-profiles', render: () => import('./pages/AgentProfilesPage.svelte') },
    { path: '/settings', render: () => import('./pages/SettingsRedirectPage.svelte') },
    { path: '/settings/common', render: () => import('./pages/SettingsCommonPage.svelte') },
    { path: '/settings/workspaces', render: () => import('./pages/WorkspacesPage.svelte') },
    { path: '/settings/agent-profiles', render: () => import('./pages/AgentProfilesPage.svelte') },
  ],
  render404: () => import('./pages/NotFoundPage.svelte'),
}
