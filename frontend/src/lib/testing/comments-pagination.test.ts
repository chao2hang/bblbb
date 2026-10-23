import { describe, expect, it, vi } from 'vitest';
import { listComments } from '$lib/api/client';

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
}

describe('listComments cursor pagination', () => {
  it('passes after and limit query params and normalizes page cursor response', async () => {
    const fetchMock = vi.fn(async (_url: string | URL | Request) => jsonResponse({
      items: [{ id: 'comment-2' }],
      page: { next_cursor: 'floor-cursor', has_more: true }
    }));

    const page = await listComments(fetchMock as typeof fetch, 'post/a', { after: 'prev cursor', limit: 10 });

    expect(String(fetchMock.mock.calls[0][0])).toContain('/posts/post%2Fa/comments?limit=10&after=prev+cursor');
    expect(page).toEqual({ items: [{ id: 'comment-2' }], next_cursor: 'floor-cursor', has_more: true });
  });

  it('supports a valid empty first page without a cursor', async () => {
    const fetchMock = vi.fn(async (_url: string | URL | Request) => jsonResponse({ items: [], page: { next_cursor: null, has_more: false } }));
    await expect(listComments(fetchMock as typeof fetch, 'post-1')).resolves.toEqual({
      items: [], next_cursor: null, has_more: false
    });
    expect(String(fetchMock.mock.calls[0][0])).toContain('/posts/post-1/comments');
    expect(String(fetchMock.mock.calls[0][0])).not.toContain('?');
  });
});
