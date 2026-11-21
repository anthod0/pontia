import type { Component } from "svelte";
import FileIcon from "phosphor-svelte/lib/FileIcon";
import FileArchiveIcon from "phosphor-svelte/lib/FileArchiveIcon";
import FileAudioIcon from "phosphor-svelte/lib/FileAudioIcon";
import FileCodeIcon from "phosphor-svelte/lib/FileCodeIcon";
import FileDocIcon from "phosphor-svelte/lib/FileDocIcon";
import FileImageIcon from "phosphor-svelte/lib/FileImageIcon";
import FilePdfIcon from "phosphor-svelte/lib/FilePdfIcon";
import FilePptIcon from "phosphor-svelte/lib/FilePptIcon";
import FileTextIcon from "phosphor-svelte/lib/FileTextIcon";
import FileVideoIcon from "phosphor-svelte/lib/FileVideoIcon";
import FileXlsIcon from "phosphor-svelte/lib/FileXlsIcon";
import FileZipIcon from "phosphor-svelte/lib/FileZipIcon";

// oxlint-disable-next-line typescript/no-explicit-any -- phosphor icon components share one prop shape
type Icon = Component<any>;
/** `text` marks kinds the file preview panel can show; binary kinds are download-only. */
export type FileKind = { icon: Icon; tone: string; text: boolean };

const kinds: [string[], FileKind][] = [
  [
    ["mp4", "mov", "webm", "mkv", "avi", "m4v"],
    { icon: FileVideoIcon, tone: "bg-interrupted/12 text-interrupted", text: false },
  ],
  [
    ["png", "jpg", "jpeg", "gif", "webp", "heic", "avif"],
    { icon: FileImageIcon, tone: "bg-aqua/12 text-aqua", text: false },
  ],
  [["svg"], { icon: FileImageIcon, tone: "bg-aqua/12 text-aqua", text: true }],
  [
    ["mp3", "wav", "m4a", "flac", "ogg", "aac"],
    { icon: FileAudioIcon, tone: "bg-interrupted/12 text-interrupted", text: false },
  ],
  [["pdf"], { icon: FilePdfIcon, tone: "bg-destructive/10 text-destructive", text: false }],
  [
    ["zip", "tar", "gz", "tgz", "7z", "rar", "xz", "zst", "bz2"],
    { icon: FileZipIcon, tone: "bg-warning/14 text-warning", text: false },
  ],
  [
    ["dmg", "iso", "deb", "rpm", "apk"],
    { icon: FileArchiveIcon, tone: "bg-warning/14 text-warning", text: false },
  ],
  [
    ["xlsx", "xls", "ods", "numbers"],
    { icon: FileXlsIcon, tone: "bg-success/14 text-success", text: false },
  ],
  [["csv", "tsv"], { icon: FileXlsIcon, tone: "bg-success/14 text-success", text: true }],
  [
    ["ppt", "pptx", "odp", "key"],
    { icon: FilePptIcon, tone: "bg-warning/14 text-warning", text: false },
  ],
  [
    ["doc", "docx", "odt", "rtf", "pages"],
    { icon: FileDocIcon, tone: "bg-primary/12 text-primary", text: false },
  ],
  [
    ["txt", "md", "markdown", "log"],
    { icon: FileTextIcon, tone: "bg-primary/12 text-primary", text: true },
  ],
  [
    [
      "json",
      "yaml",
      "yml",
      "toml",
      "xml",
      "html",
      "css",
      "js",
      "ts",
      "tsx",
      "jsx",
      "svelte",
      "py",
      "rs",
      "go",
      "java",
      "c",
      "h",
      "cpp",
      "rb",
      "sh",
      "sql",
    ],
    { icon: FileCodeIcon, tone: "bg-primary/12 text-primary", text: true },
  ],
];
// Unknown and extensionless files (Makefile, LICENSE) are often text; the preview panel reports binary content.
const fallback: FileKind = {
  icon: FileIcon,
  tone: "bg-background text-muted-foreground",
  text: true,
};

/** Icon and colour for a file, chosen by extension because shared files are mostly typed `application/octet-stream`. */
export function fileKind(extension: string): FileKind {
  const lower = extension.toLowerCase();
  return kinds.find(([extensions]) => extensions.includes(lower))?.[1] ?? fallback;
}

/** Whether the file preview panel can show the named file; other files are download-only. */
export function canPreviewText(name: string): boolean {
  const dot = name.lastIndexOf(".");
  return fileKind(dot > 0 ? name.slice(dot + 1) : "").text;
}
