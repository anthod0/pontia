import { writable } from 'svelte/store';
import { discoverArtifacts as apiDiscoverArtifacts, getArtifactContent, listArtifacts } from '../api/client';
import type { ArtifactContent, ArtifactView } from '../api/types';

export const artifacts = writable<ArtifactView[]>([]);
export const selectedArtifact = writable<ArtifactView | null>(null);
export const artifactContent = writable<ArtifactContent | null>(null);
export const artifactsLoading = writable(false);
export const artifactsError = writable<string | null>(null);

export async function loadArtifacts(sessionId: string): Promise<void> {
  artifactsLoading.set(true);
  artifactsError.set(null);
  try {
    artifacts.set(await listArtifacts(sessionId));
  } catch (error) {
    artifactsError.set(error instanceof Error ? error.message : String(error));
  } finally {
    artifactsLoading.set(false);
  }
}

export async function discoverArtifacts(sessionId: string): Promise<void> {
  artifacts.set(await apiDiscoverArtifacts(sessionId));
}

export async function loadArtifactContent(artifact: ArtifactView): Promise<void> {
  selectedArtifact.set(artifact);
  artifactContent.set(await getArtifactContent(artifact.artifact_id));
}
