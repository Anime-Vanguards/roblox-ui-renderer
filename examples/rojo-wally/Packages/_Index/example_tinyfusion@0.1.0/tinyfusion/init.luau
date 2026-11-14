-- A tiny declarative UI helper in the style of the Fusion package, for the Rojo + Wally example:
-- New(ClassName)(Properties) builds an instance, Value holds state, Computed derives from it.
local TinyFusion = {}

-- The key under which New's properties list an instance's children
TinyFusion.Children = newproxy(false)

local Value = {}
Value.__index = Value

function Value:get()
	return self._value
end

function Value:set(NewValue)
	self._value = NewValue
end

function TinyFusion.Value(Initial)
	return setmetatable({ _value = Initial }, Value)
end

function TinyFusion.Computed(Callback)
	return setmetatable({ _callback = Callback }, {
		__index = function(Self, Key)
			if Key == "get" then
				return function()
					return Self._callback()
				end
			end
			return nil
		end,
	})
end

local function Unwrap(Item)
	if type(Item) == "table" and type(Item.get) == "function" then
		return Item:get()
	end
	return Item
end

function TinyFusion.New(ClassName: string)
	return function(Properties: { [any]: any })
		local Object = Instance.new(ClassName)
		local Parent = nil
		for Key, Item in Properties do
			if Key == TinyFusion.Children then
				for _, Child in Item do
					Child.Parent = Object
				end
			elseif Key == "Parent" then
				Parent = Item
			else
				Object[Key] = Unwrap(Item)
			end
		end
		-- Parented last, as Fusion does, so the instance is complete when it appears
		Object.Parent = Parent
		return Object
	end
end

return TinyFusion
