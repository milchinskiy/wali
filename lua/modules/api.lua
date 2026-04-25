local api = {}

local apply = function()
	local obj = { message = "", changes = {} }

	function obj:with(msg)
		self.message = msg
        return self
	end

	function obj:__add_change(kind, path, detail)
		table.insert(self.changes, { kind = kind, subject = "fs_entry", path = path, detail = detail or nil })
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
		return {
			changes = self.changes,
			message = self.message,
		}
	end

	return obj
end

api.result = {
	apply = apply,
}

return api
