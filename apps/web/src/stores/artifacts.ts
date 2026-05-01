import { writable } from 'svelte/store';
import { discoverArtifacts as apiDiscoverArtifacts, getArtifactContent, listArtifacts } from '../api/client';
import type { ArtifactContent, ArtifactView } from '../api/types';

export const artifacts = writable<ArtifactView[]>([]);
export const selectedArtifact = writable<ArtifactView | null>(null);
export const artifactContent = writable<ArtifactContent | null>(null);
export const artifactsLoading = writable(false);
export const artifactContentLoading = writable(false);
export const artifactsError = writable<string | null>(null);
export const artifactContentError = writable<string | null>(null);

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function loadArtifacts(sessionId: string): Promise<void> {
  artifactsLoading.set(true);
  artifactsError.set(null);
  selectedArtifact.set(null);
  artifactContent.set(null);
  artifactContentError.set(null);
  try {
    artifacts.set(await listArtifacts(sessionId));
  } catch (error) {
    artifactsError.set(errorMessage(error));
  } finally {
    artifactsLoading.set(false);
  }
}

export async function discoverArtifacts(sessionId: string): Promise<void> {
  artifactsLoading.set(true);
  artifactsError.set(null);
  try {
    artifacts.set(await apiDiscoverArtifacts(sessionId));
    selectedArtifact.set(null);
    artifactContent.set(null);
    artifactContentError.set(null);
  } catch (error) {
    artifactsError.set(errorMessage(error));
    throw error;
  } finally {
    artifactsLoading.set(false);
  }
}

export async function loadArtifactContent(artifact: ArtifactView): Promise<void> {
  selectedArtifact.set(artifact);
  artifactContent.set(null);
  artifactContentLoading.set(true);
  artifactContentError.set(null);
  try {
    artifactContent.set(await getArtifactContent(artifact.artifact_id));
  } catch (error) {
    artifactContentError.set(errorMessage(error));
    throw error;
  } finally {
    artifactContentLoading.set(false);
  }
}
