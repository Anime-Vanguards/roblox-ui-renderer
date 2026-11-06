using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Posts mouse window messages to the Studio viewport window (no SendInput, no cursor movement): the
// message coordinates are client pixels of the viewport HWND (physical pixels).
public static class WinInput
{
	[StructLayout(LayoutKind.Sequential)]
	public struct RECT { public int Left, Top, Right, Bottom; }
	[StructLayout(LayoutKind.Sequential)]
	public struct POINT { public int X, Y; }

	[DllImport("user32.dll")] static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
	[DllImport("user32.dll")] static extern bool GetCursorPos(out POINT point);
	[DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr value);
	[DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
	[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
	delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

	public const uint WM_MOUSEMOVE = 0x0200, WM_LBUTTONDOWN = 0x0201, WM_LBUTTONUP = 0x0202, WM_RBUTTONDOWN = 0x0204, WM_RBUTTONUP = 0x0205, WM_MBUTTONDOWN = 0x0207, WM_MBUTTONUP = 0x0208, WM_MOUSEWHEEL = 0x020A, WM_MOUSEHWHEEL = 0x020E, WM_MOUSELEAVE = 0x02A3;
	public const int MK_LBUTTON = 1, MK_RBUTTON = 2, MK_MBUTTON = 0x10;

	public static void MakeDpiAware()
	{
		SetProcessDpiAwarenessContext(new IntPtr(-4));
	}

	// The last visible child window of the Studio main window whose size matches the viewport.
	public static long FindViewport(string titleMatch, int width, int height)
	{
		MakeDpiAware();
		long found = 0;
		EnumWindows((hWnd, _) =>
		{
			if (!IsWindowVisible(hWnd)) return true;
			var title = new StringBuilder(512);
			GetWindowText(hWnd, title, 512);
			if (title.ToString().IndexOf(titleMatch, StringComparison.OrdinalIgnoreCase) < 0) return true;
			if (title.ToString().IndexOf("Roblox Studio", StringComparison.OrdinalIgnoreCase) < 0) return true;
			EnumChildWindows(hWnd, (child, __) =>
			{
				RECT rect;
				GetWindowRect(child, out rect);
				int w = rect.Right - rect.Left, h = rect.Bottom - rect.Top;
				if (IsWindowVisible(child) && Math.Abs(w - width) <= 1 && Math.Abs(h - height) <= 1) found = child.ToInt64();
				return true;
			}, IntPtr.Zero);
			return found == 0;
		}, IntPtr.Zero);
		return found;
	}

	static IntPtr Point(int x, int y)
	{
		return new IntPtr((y << 16) | (x & 0xFFFF));
	}

	public static string Cursor()
	{
		POINT point;
		GetCursorPos(out point);
		return point.X + "," + point.Y;
	}

	public static string Origin(long hwnd)
	{
		RECT rect;
		GetWindowRect(new IntPtr(hwnd), out rect);
		return rect.Left + "," + rect.Top;
	}

	public static bool Move(long hwnd, int x, int y, int keys)
	{
		return PostMessage(new IntPtr(hwnd), WM_MOUSEMOVE, new IntPtr(keys), Point(x, y));
	}

	public static bool Button(long hwnd, int button, bool down, int x, int y, int keys)
	{
		uint message = button == 1 ? (down ? WM_LBUTTONDOWN : WM_LBUTTONUP) : button == 2 ? (down ? WM_RBUTTONDOWN : WM_RBUTTONUP) : (down ? WM_MBUTTONDOWN : WM_MBUTTONUP);
		return PostMessage(new IntPtr(hwnd), message, new IntPtr(keys), Point(x, y));
	}

	// Wheel messages carry screen coordinates; delta in WHEEL_DELTA (120) units.
	public static bool Wheel(long hwnd, int delta, int x, int y, int keys, bool horizontal)
	{
		RECT rect;
		GetWindowRect(new IntPtr(hwnd), out rect);
		return PostMessage(new IntPtr(hwnd), horizontal ? WM_MOUSEHWHEEL : WM_MOUSEWHEEL, new IntPtr((delta << 16) | (keys & 0xFFFF)), Point(rect.Left + x, rect.Top + y));
	}

	public static bool Leave(long hwnd)
	{
		return PostMessage(new IntPtr(hwnd), WM_MOUSELEAVE, IntPtr.Zero, IntPtr.Zero);
	}

	// Posts several messages back to back (microseconds apart), so they all sit in Studio's queue before
	// Qt handles the first one: no WM_MOUSELEAVE (queued by Qt's TrackMouseEvent because the real
	// cursor is elsewhere) gets between them. data: [op, button, x, y, keys, delta] per message with op
	// 1 = move, 2 = down, 3 = up, 4 = wheel, 5 = horizontal wheel.
	public static int PostBatch(long hwnd, int[] data)
	{
		int posted = 0;
		for (int i = 0; i + 5 < data.Length; i += 6)
		{
			int op = data[i], button = data[i + 1], x = data[i + 2], y = data[i + 3], keys = data[i + 4], delta = data[i + 5];
			bool ok = false;
			if (op == 1) ok = Move(hwnd, x, y, keys);
			else if (op == 2) ok = Button(hwnd, button, true, x, y, keys);
			else if (op == 3) ok = Button(hwnd, button, false, x, y, keys);
			else if (op == 4) ok = Wheel(hwnd, delta, x, y, keys, false);
			else if (op == 5) ok = Wheel(hwnd, delta, x, y, keys, true);
			if (ok) posted++;
		}
		return posted;
	}
}
