export {};

declare global {
  interface Window {
    Otzaria?: {
      call(method: string, payload?: Record<string, unknown>): Promise<{
        success: boolean;
        data: unknown;
        error?: { code: string; message: string } | null;
      }>;
      on(event: string, callback: (payload: any) => void): void;
      off(event: string, callback: (payload: any) => void): void;
    };
  }
}
