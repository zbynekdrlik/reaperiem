-- Set track colors for IEM project
-- Colors are in native OS format (0xBBGGRR on Windows)

local COLORS = {
    folder = 0x404040,      -- dark gray for folders
    mics = 0xD4A574,        -- warm orange for mics
    stems = 0x74D4A5,       -- green for stems  
    tech_in = 0xA574D4,     -- purple for tech inputs
    band_out = 0x74A5D4,    -- blue for band outputs
    tech_out = 0xD47474,    -- red for tech outputs
}

local function set_color(track, color)
    reaper.SetTrackColor(track, color)
end

local track_count = reaper.CountTracks(0)
for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    
    -- Folders
    if name == "INPUTS" or name == "OUTPUTS" or name == "MICS" or name == "STEMS" or name == "BAND" then
        set_color(track, COLORS.folder)
    elseif name == "TECH" then
        -- TECH folder - check position to determine input vs output
        if i < 30 then
            set_color(track, COLORS.folder)
        else
            set_color(track, COLORS.folder)
        end
    -- Mics (input)
    elseif name:match("mic$") or name:match("gtr$") then
        set_color(track, COLORS.mics)
    -- Stems
    elseif name:match("^DRUMS") or name:match("^BASS") or name:match("^INST") or 
           name:match("^OTHER") or name:match("^BGVS") or name:match("^CLICK") or 
           name:match("^GUIDE") or name:match("^IEMONLY") then
        set_color(track, COLORS.stems)
    -- Tech inputs (HAND mics)
    elseif name:match("^HAND") or name == "ENGINEER mic" then
        set_color(track, COLORS.tech_in)
    -- Band outputs (inear)
    elseif name:match("inear$") and not name:match("ENGINEER") then
        set_color(track, COLORS.band_out)
    -- Tech outputs
    elseif name == "ENGINEER inear" or name == "TRANSLATOR" then
        set_color(track, COLORS.tech_out)
    end
end

reaper.UpdateArrange()
reaper.TrackList_AdjustWindows(false)
