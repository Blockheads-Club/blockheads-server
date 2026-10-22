--!strict
-- chan-echo: answers every message on chan-echo:echo with "echo:<data>" to the sending player; /chanall sends to
-- every modded player, /chanlate opens a second channel after the mod list was sent; the alias NOPE is refused at
-- player-pre-login.
local mod = {}

function mod.on_enable()
	bh.events.on("player-pre-login", function(ev)
		if ev.alias == "NOPE" then
			ev.cancelled = true
			ev.reason = "banned"
		end
	end)
	local echo: number
	echo = bh.channels.open("chan-echo:echo", function(origin, data)
		if origin.player then
			bh.channels.send(echo, origin.player, `echo:{data}`)
		end
	end)
	bh.commands.register({ name = "chanall", description = "Channel broadcast.", usage = "/chanall" }, function()
		bh.channels.send(echo, "all-players", "to all")
		-- A server mod cannot send to "server".
		local ok = pcall(bh.channels.send, echo, "server", "x")
		return { handled = true, reply = { if ok then "sent to server?" else "sent" } }
	end)
	bh.commands.register({ name = "chanlate", description = "Opens a late channel.", usage = "/chanlate" }, function()
		local late: number
		late = bh.channels.open("chan-echo:late", function(origin, data)
			if origin.player then
				bh.channels.send(late, origin.player, `late:{data}`)
			end
		end)
		return { handled = true, reply = { "opened" } }
	end)
end

return mod
