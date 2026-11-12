// Starts Roblox Studio without showing it on any screen and without taking the user's keyboard focus
// (tools/studio/launch.mjs; loaded by launch.ps1 through Add-Type).
//
// The default puts the process on a desktop of its own (CreateDesktop, STARTUPINFO.lpDesktop): its splash, main
// window and dialogs are never on the user's desktop, so they can neither show on a monitor nor take the focus.
// Studio renders there at full rate, but Windows composes only the input desktop, so captures go through the engine
// (CaptureService, cli/studio.mjs captureEngine) instead of PrintWindow. Measured 2026-11-12: on the user's desktop
// the splash (900x450, centred) stayed on the monitor 10-12 s whatever the start options (SW_SHOWMINNOACTIVE,
// STARTF_USEPOSITION, -qwindowgeometry), because Studio's busy UI thread applies no move from outside for seconds.
//
// The user's-desktop mode (--user-desktop) is what follows:
// - CreateProcess with STARTUPINFO.wShowWindow = SW_SHOWMINNOACTIVE: windows that honour it open minimized and
//   inactive.
// - A WinEvent hook on the Studio process (EVENT_OBJECT_SHOW, EVENT_OBJECT_LOCATIONCHANGE) parks each of its top-level
//   windows as it is shown: above the primary monitor (whose display scale it keeps) at x = primary left + 40, with
//   only its bottom VISIBLE_STRIP pixels on the monitor's top row, at the bottom of the z-order, never activated.
//   A window entirely off the monitors keeps rendering, but Windows stops composing it and PrintWindow returned the
//   previous mount's frame in 5 of 16 captures (0 of 16 with the strip). A minimized window gets the parking
//   rectangle as its restore position and is restored there, so it is never drawn on a monitor any further; a
//   maximized one is restored first. The machine's Studio focus guard parks by the same rule and counts a window
//   with at most a thin strip showing as parked, so the two never move each other's windows.
// - Studio draws its own cursor at the viewport's centre until the mouse has been over the viewport; a posted
//   WM_MOUSEMOVE (no real input, the cursor does not move) puts it in the viewport's bottom-right corner.
// - If a window of the process becomes the foreground anyway, the previous foreground window gets it back.
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Forms;

public static class StudioLaunch
{
	[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
	struct STARTUPINFO
	{
		public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
		public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
		public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
	}
	[StructLayout(LayoutKind.Sequential)]
	struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
	[StructLayout(LayoutKind.Sequential)]
	public struct RECT { public int Left, Top, Right, Bottom; }
	[StructLayout(LayoutKind.Sequential)]
	struct POINT { public int X, Y; }
	[StructLayout(LayoutKind.Sequential)]
	struct WINDOWPLACEMENT { public int length, flags, showCmd; public POINT minPosition, maxPosition; public RECT normalPosition; }
	[StructLayout(LayoutKind.Sequential)]
	struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam, lParam; public uint time; public POINT pt; }
	delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
	delegate void WinEventProc(IntPtr hook, uint evt, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);

	[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
	static extern bool CreateProcessW(string app, StringBuilder cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string dir, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
	[DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
	[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
	[DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
	[DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
	[DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
	[DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsZoomed(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
	[DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
	[DllImport("user32.dll")] static extern bool GetWindowPlacement(IntPtr h, ref WINDOWPLACEMENT p);
	[DllImport("user32.dll")] static extern bool SetWindowPlacement(IntPtr h, ref WINDOWPLACEMENT p);
	[DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h, uint flags);
	[DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
	[DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
	[DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr value);
	[DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr module, WinEventProc proc, uint pid, uint thread, uint flags);
	[DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr hook);
	[DllImport("user32.dll")] static extern bool PeekMessageW(out MSG msg, IntPtr h, uint min, uint max, uint remove);
	[DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG msg);
	[DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG msg);
	[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);

	const int SW_SHOWMINNOACTIVE = 7, SW_SHOWNOACTIVATE = 4;
	const uint SWP_NOSIZE = 0x1, SWP_NOMOVE = 0x2, SWP_NOACTIVATE = 0x10;
	const uint EVENT_OBJECT_SHOW = 0x8002, EVENT_OBJECT_LOCATIONCHANGE = 0x800B, EVENT_SYSTEM_FOREGROUND = 0x0003;
	static readonly IntPtr HWND_BOTTOM = new IntPtr(1);

	public static void MakeDpiAware()
	{
		SetProcessDpiAwarenessContext(new IntPtr(-4)); // per-monitor aware v2: physical pixels
	}

	[DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
	static extern IntPtr CreateDesktopW(string name, IntPtr device, IntPtr devmode, uint flags, uint access, IntPtr sa);
	[DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
	[DllImport("user32.dll")] static extern bool EnumDesktopWindows(IntPtr desktop, EnumProc callback, IntPtr lParam);

	// Starts `exe args` inactive, on `desktop` when given (created if needed); returns the process id
	public static int Start(string exe, string args, string desktop)
	{
		STARTUPINFO si = new STARTUPINFO();
		si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
		si.dwFlags = 1; // STARTF_USESHOWWINDOW
		// Minimized on the user's desktop; normal on Studio's own, where it then maximizes itself as it loads (a
		// minimized start there kept the main window minimized, at a 794x607 viewport at scale 1)
		si.wShowWindow = (short)(String.IsNullOrEmpty(desktop) ? SW_SHOWMINNOACTIVE : SW_SHOWNOACTIVATE);
		IntPtr handle = IntPtr.Zero;
		if (!String.IsNullOrEmpty(desktop))
		{
			// The desktop lives while a process runs on it: this handle is closed once Studio is started
			handle = CreateDesktopW(desktop, IntPtr.Zero, IntPtr.Zero, 0, 0x01FF | 0x000F0000, IntPtr.Zero);
			if (handle == IntPtr.Zero) throw new Exception("CreateDesktop failed: " + Marshal.GetLastWin32Error());
			si.lpDesktop = "WinSta0\\" + desktop;
		}
		PROCESS_INFORMATION pi;
		StringBuilder cmd = new StringBuilder("\"" + exe + "\" " + args);
		bool created = CreateProcessW(exe, cmd, IntPtr.Zero, IntPtr.Zero, false, 0, IntPtr.Zero, null, ref si, out pi);
		int error = Marshal.GetLastWin32Error();
		if (handle != IntPtr.Zero)
		{
			// A desktop nobody holds is destroyed: keep it open until Studio has a window on it (Studios whose
			// launcher let go at once died before writing a log)
			Stopwatch waited = Stopwatch.StartNew();
			bool hasWindow = false;
			while (created && !hasWindow && waited.ElapsedMilliseconds < 60000)
			{
				EnumDesktopWindows(handle, (h, _) =>
				{
					if (PidOf(h) == (uint)pi.dwProcessId) hasWindow = true;
					return !hasWindow;
				}, IntPtr.Zero);
				if (!hasWindow) System.Threading.Thread.Sleep(100);
			}
			CloseDesktop(handle);
		}
		if (!created) throw new Exception("CreateProcess failed: " + error);
		CloseHandle(pi.hThread);
		CloseHandle(pi.hProcess);
		return pi.dwProcessId;
	}

	static uint PidOf(IntPtr h) { uint pid; GetWindowThreadProcessId(h, out pid); return pid; }
	static string Title(IntPtr h) { StringBuilder text = new StringBuilder(512); GetWindowText(h, text, 512); return text.ToString(); }

	public static string Describe(IntPtr h)
	{
		string name = "?";
		try { name = Process.GetProcessById((int)PidOf(h)).ProcessName; } catch (Exception) { }
		return String.Format("{0} {1} \"{2}\"", h, name, Title(h));
	}

	// Studio's main window: a visible top-level window of the process titled "... Roblox Studio"
	public static IntPtr MainWindow(int pid)
	{
		IntPtr found = IntPtr.Zero;
		EnumWindows((h, _) =>
		{
			if (PidOf(h) == (uint)pid && IsWindowVisible(h) && Title(h).EndsWith("Roblox Studio")) found = h;
			return found == IntPtr.Zero;
		}, IntPtr.Zero);
		return found;
	}

	public const int VISIBLE_STRIP = 4;
	// More than this many physical pixels of a window on a monitor is "on screen" (the guard's 6 px, DPI-unaware, at 1.5)
	const int PARKED_MAX = 9;

	static bool OnAnyMonitor(RECT r)
	{
		var rect = System.Drawing.Rectangle.FromLTRB(r.Left, r.Top, r.Right, r.Bottom);
		foreach (Screen screen in Screen.AllScreens)
		{
			var part = System.Drawing.Rectangle.Intersect(rect, screen.Bounds);
			if (!part.IsEmpty && Math.Min(part.Width, part.Height) > PARKED_MAX) return true;
		}
		return false;
	}

	// The parking rectangle for a window of width x height: above the primary monitor, VISIBLE_STRIP px showing
	static RECT ParkingRect(int width, int height)
	{
		var primary = Screen.PrimaryScreen.Bounds;
		RECT r;
		r.Left = primary.Left + 40;
		r.Top = primary.Top - height + VISIBLE_STRIP;
		r.Right = r.Left + width;
		r.Bottom = r.Top + height;
		return r;
	}

	// Parks one top-level window (nothing when it is hidden or already off every monitor). Returns what it did, or null.
	public static string Park(IntPtr h)
	{
		if (!IsWindowVisible(h)) return null;
		if (IsIconic(h))
		{
			// Restore it straight to the parking rectangle, inactive: it is never drawn on a monitor
			WINDOWPLACEMENT p = new WINDOWPLACEMENT();
			p.length = Marshal.SizeOf(typeof(WINDOWPLACEMENT));
			GetWindowPlacement(h, ref p);
			int w = p.normalPosition.Right - p.normalPosition.Left, hh = p.normalPosition.Bottom - p.normalPosition.Top;
			p.normalPosition = ParkingRect(w, hh);
			p.showCmd = SW_SHOWNOACTIVATE;
			p.flags = 0;
			SetWindowPlacement(h, ref p);
			SetWindowPos(h, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
			return "restored from minimized to " + Rect(h);
		}
		RECT r;
		GetWindowRect(h, out r);
		if (!OnAnyMonitor(r)) return null;
		if (IsZoomed(h))
		{
			ShowWindow(h, SW_SHOWNOACTIVATE);
			GetWindowRect(h, out r);
		}
		RECT to = ParkingRect(r.Right - r.Left, r.Bottom - r.Top);
		SetWindowPos(h, HWND_BOTTOM, to.Left, to.Top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
		return "moved from " + r.Left + "," + r.Top + " to " + Rect(h);
	}

	static string Rect(IntPtr h)
	{
		RECT r;
		GetWindowRect(h, out r);
		return String.Format("{0},{1} {2}x{3} dpi={4}", r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top, GetDpiForWindow(h));
	}

	// Posts a mouse move to the bottom-right corner of every large child window (the viewport is one): Studio then
	// draws its cursor there instead of at the viewport's centre. No real input; the cursor does not move.
	public static int ParkCursor(IntPtr main)
	{
		int posted = 0;
		EnumChildWindows(main, (child, _) =>
		{
			RECT r;
			GetWindowRect(child, out r);
			int w = r.Right - r.Left, hh = r.Bottom - r.Top;
			if (IsWindowVisible(child) && w >= 400 && hh >= 300)
			{
				int x = w - 2, y = hh - 2;
				PostMessageW(child, 0x0200, IntPtr.Zero, new IntPtr((y << 16) | (x & 0xFFFF))); // WM_MOUSEMOVE
				posted++;
			}
			return true;
		}, IntPtr.Zero);
		return posted;
	}

	// Watches the process for `ms` milliseconds (or until it exits): each top-level window it shows or moves onto a
	// monitor is parked at once, and a window of it that takes the foreground hands it back to `previous`. Returns a
	// log, one line per event: "ON SCREEN" marks a window that was found on a monitor before it was parked.
	public static string Guard(int pid, IntPtr previous, int ms)
	{
		StringBuilder log = new StringBuilder();
		Stopwatch clock = Stopwatch.StartNew();
		log.AppendLine("0 ms: foreground " + Describe(GetForegroundWindow()));
		WinEventProc onEvent = (hook, evt, h, idObject, idChild, thread, time) =>
		{
			if (idObject != 0 || h == IntPtr.Zero || GetAncestor(h, 2) != h) return; // GA_ROOT: top-level windows only
			if (PidOf(h) != (uint)pid) return;
			if (evt == EVENT_SYSTEM_FOREGROUND)
			{
				Park(h);
				if (previous != IntPtr.Zero && IsWindow(previous)) SetForegroundWindow(previous);
				log.AppendLine(clock.ElapsedMilliseconds + " ms: FOREGROUND TAKEN by " + Describe(h) + "; now " + Describe(GetForegroundWindow()));
				return;
			}
			bool iconic = IsIconic(h);
			string done = Park(h);
			if (done != null) log.AppendLine(clock.ElapsedMilliseconds + " ms: " + (iconic ? "parked " : "ON SCREEN, parked ") + Describe(h) + ": " + done);
		};
		IntPtr showHook = SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, IntPtr.Zero, onEvent, (uint)pid, 0, 0);
		IntPtr moveHook = SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE, IntPtr.Zero, onEvent, (uint)pid, 0, 0);
		IntPtr foregroundHook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, IntPtr.Zero, onEvent, 0, 0, 0);
		try
		{
			while (clock.ElapsedMilliseconds < ms)
			{
				MSG msg;
				while (PeekMessageW(out msg, IntPtr.Zero, 0, 0, 1))
				{
					TranslateMessage(ref msg);
					DispatchMessageW(ref msg);
				}
				bool alive;
				try { alive = !Process.GetProcessById(pid).HasExited; } catch (Exception) { alive = false; }
				if (!alive) { log.AppendLine(clock.ElapsedMilliseconds + " ms: Studio exited"); break; }
				System.Threading.Thread.Sleep(5);
			}
		}
		finally
		{
			UnhookWinEvent(showHook);
			UnhookWinEvent(moveHook);
			UnhookWinEvent(foregroundHook);
			GC.KeepAlive(onEvent);
		}
		return log.ToString();
	}
}
