export type RichPreviewSegment =
  | RichPreviewTextSegment
  | RichPreviewImageSegment
  | RichPreviewVideoSegment;

export interface RichPreviewTextSegment {
  type: "text";
  text: string;
}

export interface RichPreviewImageSegment {
  type: "image";
  label: string;
  media_type: string;
  data: number[];
}

export interface RichPreviewVideoSegment {
  type: "video";
  label: string;
  media_type: string;
}

export interface HistorySummary {
  content_hash: string;
  data_type: string;
  display: number[];
  display_truncated: boolean;
  source_bundle_id: string | null;
  is_remote_clipboard: boolean;
  timestamp: number;
  byte_count: number;
  has_detail: boolean;
}

export interface HistoryDetail {
  content_hash: string;
  html_preview: string | null;
  text_preview: string | null;
  rich_preview: RichPreviewSegment[];
}

export interface HistoryPage {
  items: HistorySummary[];
  next_cursor: string | null;
  has_more: boolean;
  total_count: number;
  total_bytes: number;
}

export interface AppSettings {
  max_items: number;
  max_history_bytes: number;
  show_in_menu_bar: boolean;
  menu_bar_item_limit: number;
  move_restored_item_to_top: boolean;
  compact_mode: boolean;
  language: string;
  resolved_language: string;
  history_count: number;
  history_bytes: number;
  history_limit_bytes: number;
  max_event_bytes: number;
}

export interface CaptureRejectedNotice {
  code?: string;
  reason?: string;
  size_bucket?: string;
}

export type ErrorCode =
  | "startup_failed"
  | "database_unavailable"
  | "database_operation_failed"
  | "history_item_not_found"
  | "clipboard_write_failed"
  | "restore_post_processing_failed"
  | "invalid_setting"
  | "invalid_history_cursor"
  | "state_unavailable"
  | "autostart_unavailable"
  | "autostart_verification_failed"
  | "history_mirror_failed"
  | "capture_rejected"
  | "unknown";

export type Operation =
  | "startup"
  | "capture_clipboard"
  | "load_history"
  | "load_history_detail"
  | "restore_clipboard"
  | "delete_history"
  | "clear_history"
  | "load_settings"
  | "update_settings"
  | "update_autostart"
  | "write_history_mirror";

export interface CommandError {
  code: ErrorCode;
  operation: Operation;
  retryable: boolean;
}

export interface SafeDiagnostic {
  timestamp: number;
  version: string;
  platform: string;
  architecture: string;
  code: ErrorCode;
  operation: Operation;
  retryable: boolean;
}
