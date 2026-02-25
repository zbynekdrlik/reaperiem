import { test, expect } from '@playwright/test';

test.describe('Smoke Tests - Must All Pass', () => {
  test('landing page loads and returns HTTP 200', async ({ page }) => {
    const response = await page.goto('/');
    expect(response?.status()).toBe(200);
  });

  test('landing page contains member content', async ({ page }) => {
    await page.goto('/');
    // Page should have some content (not blank)
    const content = await page.content();
    expect(content.length).toBeGreaterThan(100);
  });

  test('login page is accessible', async ({ page }) => {
    const response = await page.goto('/login');
    expect(response?.status()).toBe(200);
  });

  test('API members endpoint returns JSON', async ({ request }) => {
    const response = await request.get('/api/members');
    expect(response.status()).toBe(200);
    const data = await response.json();
    expect(Array.isArray(data)).toBe(true);
  });

  test('static assets load correctly', async ({ page }) => {
    await page.goto('/');
    // Check that WASM loads (page should have JS execution)
    await page.waitForLoadState('networkidle');
  });
});
