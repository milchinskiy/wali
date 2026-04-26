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

	function obj:change(kind, subject, data)
		data = data or {}
		local change = {
			kind = kind,
			subject = subject,
			path = data.path,
			detail = data.detail,
		}
		table.insert(self._changes, change)
		return self
	end

	function obj:fs(kind, path, detail)
		return self:change(kind, "fs_entry", { path = path, detail = detail })
	end

	function obj:command(kind, detail)
		return self:change(kind, "command", { detail = detail })
	end

	function obj:created(path, detail)
		return self:fs("created", path, detail)
	end

	function obj:updated(path, detail)
		return self:fs("updated", path, detail)
	end

	function obj:removed(path, detail)
		return self:fs("removed", path, detail)
	end

	function obj:unchanged(path, detail)
		return self:fs("unchanged", path, detail)
	end

	function obj:merge(result)
		if result == nil then
			return self
		end
		if result.changes ~= nil then
			for _, change in ipairs(result.changes) do
				table.insert(self._changes, change)
			end
		end
		if self._message == "" and result.message ~= nil then
			self._message = result.message
		end
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
