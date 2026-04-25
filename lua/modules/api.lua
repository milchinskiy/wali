local api = {}

local validation = function()
	local obj = { _ok = false, _message = nil }

	function obj:ok(message)
		self._ok = true
		self._message = message or nil
		return self
	end

	function obj:fail(message)
		self._ok = false
		self._message = message or nil
		return self
	end

    function obj:message(msg)
        self._message = msg
        return self
    end

	function obj:build()
		return {
			ok = self._ok,
			message = self._message,
		}
	end

    return obj
end

local apply = function()
	local obj = { _message = "", _changes = {} }

	function obj:message(msg)
		self._message = msg
		return self
	end

	function obj:__add_change(kind, path, detail)
		table.insert(self._changes, { kind = kind, subject = "fs_entry", path = path, detail = detail or nil })
	end

	function obj:created(path, detail)
		self:__add_change("created", path, detail)
		return self
	end

	function obj:updated(path, detail)
		self:__add_change("updated", path, detail)
		return self
	end

	function obj:removed(path, detail)
		self:__add_change("removed", path, detail)
		return self
	end

	function obj:unchanged(path, detail)
		self:__add_change("unchanged", path, detail)
		return self
	end

	function obj:build()
		local message = self._message
		if message == "" then
			message = nil
		end
		return {
			changes = self._changes,
			message = message,
		}
	end

	return obj
end

api.result = {
	apply = apply,
	validation = validation,
}

return api
