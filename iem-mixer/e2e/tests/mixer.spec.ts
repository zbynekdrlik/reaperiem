import { test, expect } from '@playwright/test';

test.describe('Mixer Features - Must All Pass', () => {
  test('member route redirects or serves content', async ({ page }) => {
    // Member routes should either redirect to login or show mixer
    const response = await page.goto('/petka');
    expect(response?.status()).toBe(200);
  });

  test('unknown routes return valid response', async ({ page }) => {
    // SPA should handle unknown routes gracefully
    const response = await page.goto('/unknown-route-12345');
    expect(response?.status()).toBe(200);
  });

  test('API mixer endpoint responds', async ({ request }) => {
    // Mixer endpoint should respond (may be 401 without auth)
    const response = await request.get('/api/mixer/petka');
    // Either 200 (success) or 401 (unauthorized) are valid
    expect([200, 401]).toContain(response.status());
  });

  test('mobile viewport renders without errors', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    const response = await page.goto('/');
    expect(response?.status()).toBe(200);
    // No console errors
    const errors: string[] = [];
    page.on('console', msg => {
      if (msg.type() === 'error') {
        errors.push(msg.text());
      }
    });
    await page.waitForLoadState('networkidle');
    // Filter out expected WASM-related console messages
    const realErrors = errors.filter(e => !e.includes('wasm'));
    expect(realErrors).toHaveLength(0);
  });
});
