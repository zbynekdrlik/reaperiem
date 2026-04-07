-- revert_project.lua
-- Reloads the current REAPER project from its last saved state on disk.
-- Used by CI to restore REAPER state after E2E tests modify sends/FX/volumes.
--
-- IMPORTANT: CI must save the project (action 40026) BEFORE calling this script
-- to clear the dirty flag. Otherwise REAPER shows a "Save before closing?" dialog
-- that blocks the HTTP API indefinitely.
--
-- CI flow:
-- 1. Before E2E: Save project + copy .RPP to .RPP.e2e-bak
-- 2. Run E2E tests (may modify REAPER state)
-- 3. After E2E:  Save (clears dirty flag) + copy .RPP.e2e-bak back + trigger this script
--
-- Action ID: Dynamically registered via meter_bridge (numeric ID)
-- Trigger: curl "http://iem.lan:8080/_/<numeric_action_id>"
-- Result: EXTSTATE reaperiem/revert_result → "OK:<path>" or "ERROR:no_project"

local _, project_path = reaper.EnumProjects(-1)
if project_path ~= "" then
  reaper.Main_openProject(project_path)
  reaper.SetExtState("reaperiem", "revert_result", "OK:" .. project_path, false)
else
  reaper.SetExtState("reaperiem", "revert_result", "ERROR:no_project", false)
end
