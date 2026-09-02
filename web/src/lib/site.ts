// The site's constants: the origin the canonical URLs and the sitemap use.
export const SITE = "https://receiverproxy.com";
export const REPO: string = (import.meta.env.VITE_RXP_REPO as string | undefined) ?? "https://github.com/AlecDusheck/receiverproxy";
export const BRANCH: string = (import.meta.env.VITE_RXP_BRANCH as string | undefined) ?? "main";

/** A repository file, linked to its page on GitHub. */
export const repoFile = (path: string) => `${REPO}/blob/${BRANCH}/${path}`;

/** A route's `<title>`: the route's name, then the site's. */
export const title = (t: string) => `${t} · receiverproxy`;
