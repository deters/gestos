# Stands in for gestosd on the private bus, so that the app shows gestures as running.
import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

NODE = """<node><interface name='io.github.deters.Gestos.Daemon'>
<method name='Reload'/><method name='Stop'/>
<property name='Version' type='s' access='read'/>
</interface></node>"""


def call(_conn, _sender, _path, _iface, _method, _args, invocation):
    invocation.return_value(None)


def get_property(_conn, _sender, _path, _iface, _name):
    return GLib.Variant("s", "0")


def acquired(conn, _name):
    interface = Gio.DBusNodeInfo.new_for_xml(NODE).interfaces[0]
    conn.register_object("/io/github/deters/Gestos/Daemon", interface, call, get_property, None)


Gio.bus_own_name(Gio.BusType.SESSION, "io.github.deters.Gestos.Daemon", 0, acquired, None, None)
GLib.MainLoop().run()
