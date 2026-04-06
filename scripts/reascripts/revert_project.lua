-- revert_project.lua
-- Reloads the current REAPER project from its last saved state on disk.
-- Used by CI to restore REAPER state after E2E tests modify sends/FX/volumes.
--
-- Action ID: _RS_REAPERIEM_REVERT_PROJECT
-- Trigger: curl "http://iem.lan:8080/_/_RS_REAPERIEM_REVERT_PROJECT"
-- Result: EXTSTATE reaperiem/revert_result → "OK:<path>" or "ERROR:no_project"

local _, project_path = reaper.EnumProjects(-1)
if project_path ~= "" then
  reaper.Main_openProject(project_path)
  reaper.SetExtState("reaperiem", "revert_result", "OK:" .. project_path, false)
else
  reaper.SetExtState("reaperiem", "revert_result", "ERROR:no_project", false)
end
