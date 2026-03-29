import * as http from 'http';

const CHORUS_API = process.env.CHORUS_API_URL ?? 'http://localhost:3340';

export interface SearchResult {
  source: string;
  role: string;
  timestamp: string;
  content: string;
  score?: number;
}

export interface SearchResponse {
  results: SearchResult[];
  total: number;
  query: string;
}

/**
 * Search the Chorus index for messages matching a term.
 */
export function search(term: string, limit = 20): Promise<SearchResponse> {
  const url = `${CHORUS_API}/api/chorus/search?q=${encodeURIComponent(term)}&limit=${limit}`;

  return new Promise((resolve, reject) => {
    http.get(url, (res) => {
      let data = '';
      res.on('data', (chunk) => { data += chunk; });
      res.on('end', () => {
        try {
          const parsed = JSON.parse(data);
          resolve({
            results: parsed.results ?? parsed.messages ?? [],
            total: parsed.total ?? parsed.count ?? 0,
            query: term,
          });
        } catch (err) {
          reject(new Error(`Failed to parse Chorus API response: ${err}`));
        }
      });
    }).on('error', (err) => {
      reject(new Error(`Chorus API request failed: ${err.message}`));
    });
  });
}
