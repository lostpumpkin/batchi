import time
from playwright.sync_api import sync_playwright

def run(playwright):
    browser = playwright.chromium.launch()
    page = browser.new_page()
    page.goto("http://localhost:8080")

    # Wait for the app to load
    page.wait_for_selector(".app")

    # 1. Click "Load demo" button to load a file
    # The button is in the .drop-hint area initially
    try:
        page.click(".demo-btn", timeout=5000)
        print("Clicked 'Load demo'")
    except:
        print("Could not find/click 'Load demo' button. Maybe files are already loaded?")

    # Wait for file list to populate
    # The file item has class "file-item"
    try:
        page.wait_for_selector(".file-item", timeout=10000)
        print("File loaded")
    except:
        print("Timed out waiting for file to load")
        page.screenshot(path="verification/failed_load.png")
        browser.close()
        return

    # 2. Switch to Waterfall mode
    # The View Mode selector is in the sidebar (AnalysisPanel / SettingsPanel)
    # Sidebar tab "Display" must be active first?
    # By default "Files" tab is active.
    # We need to click "Display" tab in sidebar.

    # Find sidebar tab with text "Display"
    # The button text is "Display"
    display_tab = page.get_by_role("button", name="Display")
    display_tab.click()
    print("Clicked Display tab")

    # Now wait for the "View Mode" select
    page.wait_for_selector("select.setting-select")

    # Select "3D Waterfall" (value="webgl")
    # There are multiple selects in the panel (View Mode, Movement Algorithm)
    # We need the first one or the one with specific options.
    # Let's target by value.

    # Helper to select by value since there might be multiple selects
    # View Mode is the first setting group usually.
    selects = page.locator("select.setting-select").all()
    view_mode_select = None
    for s in selects:
        if "3D Waterfall" in s.inner_text():
            view_mode_select = s
            break

    if view_mode_select:
        view_mode_select.select_option(value="webgl")
        print("Selected Waterfall mode")
    else:
        print("Could not find View Mode selector")

    # Wait a bit for WebGL to init and render
    time.sleep(2)

    # 3. Take screenshot
    page.screenshot(path="verification/waterfall_mode.png")
    print("Screenshot saved to verification/waterfall_mode.png")

    browser.close()

with sync_playwright() as playwright:
    run(playwright)
