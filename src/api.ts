import { invoke } from "@tauri-apps/api/core";
import type {
  BootstrapInfo,
  DocumentPage,
  PreviewDescriptor,
  Repository,
  SearchHit
} from "./types";

export async function bootstrap(): Promise<BootstrapInfo> {
  return invoke("get_bootstrap");
}

export async function listRepositories(): Promise<Repository[]> {
  return invoke("list_repositories");
}

export async function addRepository(path: string, displayName?: string): Promise<Repository> {
  return invoke("add_repository", { rootPath: path, displayName: displayName ?? null });
}

export async function scanRepository(repositoryId: string): Promise<void> {
  await invoke("start_repository_scan", { repositoryId });
}

export async function cancelScan(repositoryId: string): Promise<void> {
  await invoke("cancel_repository_scan", { repositoryId });
}

export async function listDocuments(args: {
  repositoryIds: string[];
  query: string;
  format: string;
  offset: number;
  limit: number;
}): Promise<DocumentPage> {
  return invoke("list_documents", args);
}

export async function preparePreview(documentId: string): Promise<PreviewDescriptor> {
  return invoke("prepare_document_preview", { documentId });
}

export async function exportPdf(documentId: string, outputPath: string): Promise<void> {
  await invoke("export_document_pdf", { documentId, outputPath });
}

export async function indexDocument(documentId: string): Promise<number> {
  return invoke("index_document_text", { documentId });
}

export async function searchLibrary(args: {
  query: string;
  repositoryIds: string[];
  limit: number;
}): Promise<SearchHit[]> {
  return invoke("search_library", args);
}

export async function openLocalPath(path: string): Promise<void> {
  await invoke("open_local_path", { path });
}
