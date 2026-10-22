--!strict
-- chan-echo client half (run by modded clients; the test client only downloads it).
local mod = {}
function mod.on_enable()
	bh.channels.open("chan-echo:echo", function(_origin, data)
		bh.log.info(`server said {data}`)
	end)
end
return mod
