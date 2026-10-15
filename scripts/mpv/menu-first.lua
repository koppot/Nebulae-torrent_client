-- Nebulae: disc playback must START ON THE ROOT MENU.
-- Some discs auto-run a first-play program chain that jumps straight into
-- the feature; when the app launches with menu mode, this fires once the
-- disc is loaded and returns to the root menu, where navigation (mouse or
-- arrow keys) picks the title. Harmless no-op outside dvdnav playback.
-- mpv renamed the navigation command across releases (discnav/dvdnav) and
-- dropped it entirely in 0.40+, so try every known spelling and give up
-- silently where menu support no longer exists.
mp.register_event('file-loaded', function()
    for _, cmd in ipairs({ 'discnav', 'dvdnav' }) do
        local ok = pcall(function()
            mp.commandv(cmd, 'menu')
        end)
        if ok then
            return
        end
    end
end)
