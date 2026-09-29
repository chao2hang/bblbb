import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ url }) => {
  const target = url.searchParams.get('target') || url.searchParams.get('url') || '';
  return {
    target
  };
};
