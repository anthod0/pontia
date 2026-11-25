export const securityHeaders = {
  "X-Content-Type-Options": "nosniff",
  // Native authorization form POSTs need their Origin for CSRF validation.
  "Referrer-Policy": "strict-origin",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
  "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
  "X-Frame-Options": "DENY",
  "Content-Security-Policy":
    "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' https: data:; connect-src 'self' wss:; object-src 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors 'none'",
};

export function secureResponse(request: Request, response: Response) {
  const result = new Response(response.body, response);
  for (const [name, value] of Object.entries(securityHeaders)) {
    if (name === "Strict-Transport-Security" && !request.url.startsWith("https:")) continue;
    // SvelteKit contributes its own script nonce on SSR pages and a hash meta tag on static pages.
    if (name === "Content-Security-Policy" && response.headers.has(name)) {
      if (response.headers.get(name) !== value) result.headers.append(name, value);
      continue;
    }
    result.headers.set(name, value);
  }
  return result;
}
