local wali = {}

local function fail(message)
	error(message, 2)
end

local function validate_identifier(value, field)
	if value == "" then
		fail(field .. " must not be empty")
	end
	if value:sub(1, 1) == "." or value:sub(-1) == "." or value:find("..", 1, true) ~= nil then
		fail(field .. " contains an empty identifier")
	end
	for part in value:gmatch("[^.]+") do
		if part:match("^[0-9A-Za-z-]+$") == nil then
			fail(field .. " contains invalid identifier: " .. part)
		end
		if part:match("^%d+$") ~= nil and #part > 1 and part:sub(1, 1) == "0" then
			fail(field .. " numeric identifier must not contain leading zeroes: " .. part)
		end
	end
end

local function parse_version(value)
	if type(value) ~= "string" then
		fail("version must be a string")
	end

	local major, minor, patch, rest = value:match("^(%d+)%.(%d+)%.(%d+)(.*)$")
	if major == nil then
		major, minor, rest = value:match("^(%d+)%.(%d+)(.*)$")
		patch = "0"
	end
	if major == nil then
		major, rest = value:match("^(%d+)(.*)$")
		minor = "0"
		patch = "0"
	end
	if major == nil then
		fail("version must start with MAJOR[.MINOR[.PATCH]]")
	end

	local prerelease = nil
	local build = nil
	if rest ~= "" then
		if rest:sub(1, 1) == "-" then
			local suffix = rest:sub(2)
			local plus = suffix:find("+", 1, true)
			if plus ~= nil then
				prerelease = suffix:sub(1, plus - 1)
				build = suffix:sub(plus + 1)
			else
				prerelease = suffix
			end
			validate_identifier(prerelease, "version prerelease")
			if build ~= nil then
				validate_identifier(build, "version build metadata")
			end
		elseif rest:sub(1, 1) == "+" then
			build = rest:sub(2)
			validate_identifier(build, "version build metadata")
		else
			fail("version contains invalid suffix: " .. value)
		end
	end

	return {
		major = tonumber(major),
		minor = tonumber(minor),
		patch = tonumber(patch),
		prerelease = prerelease,
		build = build,
		text = value,
	}
end

local function is_numeric_identifier(value)
	return value:match("^%d+$") ~= nil
end

local function compare_prerelease(left, right)
	if left == nil and right == nil then
		return 0
	end
	if left == nil then
		return 1
	end
	if right == nil then
		return -1
	end

	local left_parts = {}
	local right_parts = {}
	for part in left:gmatch("[^.]+") do
		table.insert(left_parts, part)
	end
	for part in right:gmatch("[^.]+") do
		table.insert(right_parts, part)
	end

	local max = math.max(#left_parts, #right_parts)
	for idx = 1, max do
		local left_part = left_parts[idx]
		local right_part = right_parts[idx]
		if left_part == nil then
			return -1
		end
		if right_part == nil then
			return 1
		end

		local left_numeric = is_numeric_identifier(left_part)
		local right_numeric = is_numeric_identifier(right_part)
		if left_numeric and right_numeric then
			local left_number = tonumber(left_part)
			local right_number = tonumber(right_part)
			if left_number < right_number then
				return -1
			end
			if left_number > right_number then
				return 1
			end
		elseif left_numeric then
			return -1
		elseif right_numeric then
			return 1
		else
			if left_part < right_part then
				return -1
			end
			if left_part > right_part then
				return 1
			end
		end
	end

	return 0
end

local function compare_versions(left, right)
	left = type(left) == "table" and left or parse_version(left)
	right = type(right) == "table" and right or parse_version(right)

	for _, field in ipairs({ "major", "minor", "patch" }) do
		if left[field] < right[field] then
			return -1
		end
		if left[field] > right[field] then
			return 1
		end
	end
	return compare_prerelease(left.prerelease, right.prerelease)
end

local function split_comparator(term)
	for _, op in ipairs({ ">=", "<=", "==", "!=", "~=", ">", "<", "=" }) do
		if term:sub(1, #op) == op then
			return op, term:sub(#op + 1)
		end
	end
	return "==", term
end

local function satisfies_comparator(current, op, wanted)
	local cmp = compare_versions(current, wanted)
	if op == ">=" then
		return cmp >= 0
	end
	if op == ">" then
		return cmp > 0
	end
	if op == "<=" then
		return cmp <= 0
	end
	if op == "<" then
		return cmp < 0
	end
	if op == "=" or op == "==" then
		return cmp == 0
	end
	if op == "!=" or op == "~=" then
		return cmp ~= 0
	end
	fail("unsupported version comparator: " .. tostring(op))
end

local function set_version(value)
	wali.version = value
	wali.version_info = parse_version(value)
end

function wali.parse_version(value)
	return parse_version(value)
end

function wali.compare_versions(left, right)
	return compare_versions(left, right)
end

function wali.compatible(requirement, version)
	if type(requirement) ~= "string" or requirement:match("%S") == nil then
		fail("version requirement must not be empty")
	end

	local current = parse_version(version or wali.version)
	for term in requirement:gmatch("%S+") do
		local op, wanted = split_comparator(term)
		if wanted == "" then
			fail("version comparator is missing a version: " .. term)
		end
		if not satisfies_comparator(current, op, parse_version(wanted)) then
			return false
		end
	end
	return true
end

function wali.require_version(requirement, label)
	if wali.compatible(requirement) then
		return true
	end
	local subject = label or "module"
	fail(subject .. " requires wali " .. requirement .. "; current wali version is " .. wali.version)
end

function wali._set_version(value)
	set_version(value)
end

set_version("0.0.0")

return wali
