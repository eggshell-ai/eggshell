import apiService from './apiService';

/**
 * Client for the PDF export module.
 *
 * The endpoints live under `/api/pdf` and are authenticated, so requests go
 * through the shared `apiService` instance: it attaches the bearer token and
 * turns a 401 into a redirect to the login page. Building a URL and calling
 * `window.open` directly would skip the token and fail.
 */
export type PdfTemplate = {
  name: string;
  description?: string;
};

const pdfClient = {
  /** Lists the templates that have a provider registered on the backend. */
  listTemplates: async (): Promise<string[]> => {
    const response = await apiService.get<{ templates: string[] }>('/pdf/templates');
    return response.templates ?? [];
  },

  /**
   * Fetches a rendered PDF and opens it in a new tab, where the browser's PDF
   * viewer handles printing and saving.
   */
  openPdf: async (template: string, id: number | string): Promise<void> => {
    await apiService.openPdf(`/pdf/${encodeURIComponent(template)}/${encodeURIComponent(String(id))}`);
  },

  /**
   * Fetches a rendered PDF as a blob, for callers that want to upload it or
   * save it under a filename of their own choosing.
   */
  fetchPdf: async (template: string, id: number | string): Promise<Blob> => {
    const response = await apiService.get<Blob>(
      `/pdf/${encodeURIComponent(template)}/${encodeURIComponent(String(id))}`,
      { responseType: 'blob' },
    );
    return response as unknown as Blob;
  },
};

export default pdfClient;
