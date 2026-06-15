#!/usr/bin/env python3
"""
PSKK IBus Engine - Python Client
A thin wrapper that connects IBus to the PSKK Rust gRPC server.
"""

import gi
gi.require_version('IBus', '1.0')
from gi.repository import IBus, GLib
import grpc
import sys
import logging
import subprocess
import time
from pathlib import Path

# Add paths for gRPC stubs
script_dir = Path(__file__).parent
sys.path.insert(0, str(script_dir / 'proto'))  # Dev: ./proto/
sys.path.insert(0, str(script_dir))  # Installed: /opt/pskk/libexec/

# Import generated gRPC stubs
try:
    import pskk_pb2
    import pskk_pb2_grpc
except ImportError as e:
    print(f"Error: gRPC stubs not found: {e}")
    print("Run: python -m grpc_tools.protoc -I./proto --python_out=./proto --grpc_python_out=./proto ./proto/pskk.proto")
    sys.exit(1)

# Set up logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger('pskk-ibus')


class PSKKEngine(IBus.Engine):
    """
    PSKK IBus Engine - forwards all events to Rust gRPC server
    """
    
    __gtype_name__ = 'PSKKEngine'
    
    def __init__(self):
        super().__init__()
        logger.info("Initializing PSKK Engine")
        
        # Connect to gRPC server (auto-start if needed)
        self.channel = None
        self.stub = None
        self.server_process = None
        self.connect_to_server()
        
        # Property list for the input mode menu
        self._prop_list = self._create_properties()
        
        # Track Super key state manually (IBus doesn't always include it in modifier mask)
        self._super_pressed = False
        
        # Track current mode for detecting mode changes
        self._current_mode = pskk_pb2.HIRAGANA
        
        # Initialize server to Hiragana mode (default)
        if self.stub:
            logger.info("Setting initial mode to Hiragana")
            self._set_mode(pskk_pb2.HIRAGANA)
    
    def _is_server_running(self):
        """Check if the gRPC server is already running"""
        try:
            channel = grpc.insecure_channel('localhost:50051')
            stub = pskk_pb2_grpc.PSKKServiceStub(channel)
            # Try a quick health check
            stub.GetMode(pskk_pb2.Empty(), timeout=0.5)
            channel.close()
            return True
        except:
            return False
    
    def _start_server(self):
        """Start the pskk-server process"""
        # Try to find the server binary
        # Resolve the script's actual location (follow symlinks)
        script_path = Path(__file__).resolve()
        
        server_paths = [
            '/opt/pskk/bin/pskk-server',  # Installed location
            script_path.parent.parent / 'target/release/pskk-server',  # Dev build (go up from libexec)
            script_path.parent.parent / 'target/debug/pskk-server',  # Dev debug build
            script_path.parent / 'target/release/pskk-server',  # If running from project root
            script_path.parent / 'target/debug/pskk-server',  # If running from project root
        ]
        
        server_binary = None
        logger.debug(f"Searching for pskk-server binary...")
        for path in server_paths:
            logger.debug(f"  Checking: {path}")
            if Path(path).exists():
                server_binary = str(path)
                logger.info(f"  Found: {server_binary}")
                break
        
        if not server_binary:
            logger.error("Could not find pskk-server binary")
            logger.error("Tried paths:")
            for p in server_paths:
                logger.error(f"  - {p}")
            return False
        
        try:
            logger.info(f"Starting pskk-server: {server_binary}")
            # Start the server process in the background
            self.server_process = subprocess.Popen(
                [server_binary],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True  # Detach from parent
            )
            
            # Wait a bit for the server to start
            time.sleep(0.5)
            
            # Check if it's running
            if self.server_process.poll() is not None:
                # Process exited immediately
                stderr = self.server_process.stderr.read().decode('utf-8')
                logger.error(f"Server failed to start: {stderr}")
                return False
            
            logger.info("pskk-server started successfully")
            return True
            
        except Exception as e:
            logger.error(f"Failed to start pskk-server: {e}")
            return False
        
    def connect_to_server(self):
        """Connect to the PSKK gRPC server, starting it if necessary"""
        # Check if server is already running
        if self._is_server_running():
            logger.info("pskk-server is already running")
        else:
            logger.info("pskk-server not running, starting it...")
            if not self._start_server():
                logger.error("Failed to start pskk-server")
                return
            
            # Wait a bit more and retry connection
            time.sleep(0.5)
        
        # Connect to the server
        try:
            self.channel = grpc.insecure_channel('localhost:50051')
            self.stub = pskk_pb2_grpc.PSKKServiceStub(self.channel)
            
            # Verify connection
            self.stub.GetMode(pskk_pb2.Empty(), timeout=1.0)
            logger.info("✓ Connected to PSKK gRPC server at localhost:50051")
            
        except Exception as e:
            logger.error(f"✗ Failed to connect to PSKK gRPC server: {e}")
            logger.error("  The server may not be running properly")
    
    def _create_properties(self):
        """Create the property menu for input mode switching"""
        prop_list = IBus.PropList()
        
        # Input mode property
        mode_prop = IBus.Property(
            key='InputMode',
            prop_type=IBus.PropType.MENU,
            label=IBus.Text.new_from_string('あ'),
            symbol=IBus.Text.new_from_string('あ'),
            tooltip=IBus.Text.new_from_string('Input Mode')
        )
        
        # Mode menu
        mode_menu = IBus.PropList()
        
        # Hiragana mode
        hiragana = IBus.Property(
            key='InputMode.Hiragana',
            prop_type=IBus.PropType.RADIO,
            label=IBus.Text.new_from_string('Hiragana'),
            state=IBus.PropState.CHECKED
        )
        mode_menu.append(hiragana)
        
        # Alphanumeric mode
        alpha = IBus.Property(
            key='InputMode.Alphanumeric',
            prop_type=IBus.PropType.RADIO,
            label=IBus.Text.new_from_string('Alphanumeric')
        )
        mode_menu.append(alpha)
        
        mode_prop.set_sub_props(mode_menu)
        prop_list.append(mode_prop)
        
        # Settings property
        settings_prop = IBus.Property(
            key='Settings',
            prop_type=IBus.PropType.NORMAL,
            label=IBus.Text.new_from_string('Settings'),
            symbol=IBus.Text.new_from_string('⚙'),
            tooltip=IBus.Text.new_from_string('Open PSKK Settings')
        )
        prop_list.append(settings_prop)
        
        # Dictionary Editor property
        dict_editor_prop = IBus.Property(
            key='DictionaryEditor',
            prop_type=IBus.PropType.NORMAL,
            label=IBus.Text.new_from_string('Dictionary Editor'),
            symbol=IBus.Text.new_from_string('📖'),
            tooltip=IBus.Text.new_from_string('Open PSKK Dictionary Editor')
        )
        prop_list.append(dict_editor_prop)
        
        return prop_list
    
    def do_focus_in(self):
        """Called when the engine gains focus"""
        logger.debug("Focus in")
        self.register_properties(self._prop_list)
    
    def do_focus_out(self):
        """Called when the engine loses focus"""
        logger.debug("Focus out")
        if self.stub:
            try:
                response = self.stub.FocusOut(pskk_pb2.Empty())
                self._update_ui(response)
            except Exception as e:
                logger.error(f"FocusOut error: {e}")
    
    def do_reset(self):
        """Called when the engine is reset"""
        logger.debug("Reset")
        if self.stub:
            try:
                self.stub.Reset(pskk_pb2.Empty())
            except Exception as e:
                logger.error(f"Reset error: {e}")
    
    def do_enable(self):
        """Called when the engine is enabled"""
        logger.info("Engine enabled")
    
    def do_disable(self):
        """Called when the engine is disabled"""
        logger.info("Engine disabled")
    
    def do_property_activate(self, prop_name, state):
        """Handle property menu activation"""
        logger.info(f"Property activated: {prop_name}, state: {state}")
        
        # For radio buttons, only process when checked (state=1)
        # IBus calls this twice: once for the new selection (state=1) and once for the old (state=0)
        if prop_name == 'InputMode.Hiragana' and state == IBus.PropState.CHECKED:
            self._set_mode(pskk_pb2.HIRAGANA)
        elif prop_name == 'InputMode.Alphanumeric' and state == IBus.PropState.CHECKED:
            self._set_mode(pskk_pb2.ALPHANUMERIC)
        elif prop_name == 'Settings':
            self._open_settings()
        elif prop_name == 'DictionaryEditor':
            self._open_dictionary_editor()
    
    def _set_mode(self, mode):
        """Set input mode via gRPC"""
        if self.stub:
            try:
                request = pskk_pb2.SetModeRequest(mode=mode)
                response = self.stub.SetMode(request)
                self._update_ui(response)
                
                # Update property menu
                symbol = 'あ' if mode == pskk_pb2.HIRAGANA else 'A'
                self._prop_list.get(0).set_symbol(IBus.Text.new_from_string(symbol))
                self.update_property(self._prop_list.get(0))
            except Exception as e:
                logger.error(f"SetMode error: {e}")
    
    def _open_settings(self):
        """Open PSKK settings application"""
        try:
            subprocess.Popen(['pskk-settings'])
        except Exception as e:
            logger.error(f"Failed to open settings: {e}")
    
    def _open_dictionary_editor(self):
        """Open PSKK dictionary editor application"""
        try:
            subprocess.Popen(['pskk-dictionary-editor'])
        except Exception as e:
            logger.error(f"Failed to open dictionary editor: {e}")
    
    def do_process_key_event(self, keyval, keycode, state):
        """
        Main key event handler - forwards to gRPC server
        Returns True if the key was handled, False otherwise
        """
        # Check if this is a key press or release (important for simultaneous typing)
        is_pressed = not bool(state & IBus.ModifierType.RELEASE_MASK)
        
        # Convert IBus key event to PSKK KeyEvent
        key_char = chr(keyval) if 32 <= keyval < 127 else ""
        key_name = IBus.keyval_name(keyval) or str(keyval)
        
        # Track Super key state manually (IBus doesn't include it in modifier mask immediately)
        if key_name in ['Super_L', 'Super_R']:
            if is_pressed:
                self._super_pressed = True
            else:
                self._super_pressed = False
        
        # If Super is held, pass through immediately (for system shortcuts like Super+Space)
        if self._super_pressed:
            logger.info(f"Super key held, passing through: {key_name}")
            return False  # Let IBus/system handle it
        
        # Modifier keys
        shift = bool(state & IBus.ModifierType.SHIFT_MASK)
        ctrl = bool(state & IBus.ModifierType.CONTROL_MASK)
        alt = bool(state & IBus.ModifierType.MOD1_MASK)
        super_key = bool(state & IBus.ModifierType.SUPER_MASK)
        
        logger.debug(f"Key: {key_name} (char={key_char}, is_pressed={is_pressed}, shift={shift}, ctrl={ctrl}, alt={alt}, super={super_key})")
        
        if not self.stub:
            logger.warning("No gRPC connection, key not processed")
            return False
        
        try:
            # Create gRPC request
            modifiers = pskk_pb2.KeyModifiers(
                shift=shift,
                ctrl=ctrl,
                alt=alt,
                super=super_key
            )
            
            request = pskk_pb2.KeyEvent(
                key_char=key_char,
                key_name=key_name,
                is_pressed=is_pressed,
                modifiers=modifiers
            )
            
            # Send to server
            response = self.stub.ProcessKey(request)
            
            # Update UI based on response
            self._update_ui(response)
            
            # Return whether the key was consumed
            return response.consumed
            
        except Exception as e:
            logger.error(f"ProcessKey error: {e}")
            return False
    
    def _update_ui(self, output):
        """Update IBus UI based on EngineOutput from gRPC server"""
        logger.info(f"=== Python _update_ui ===")
        logger.info(f"  commit_string: {output.commit_string!r}")
        logger.info(f"  preedit_segments: {len(output.preedit_segments)}")
        logger.info(f"  candidates: {len(output.candidates)}")
        logger.info(f"  show_candidates: {output.show_candidates}")
        
        # Check if mode changed (e.g., via Henkan/Muhenkan keybinding)
        if output.current_mode != self._current_mode:
            logger.info(f"Mode changed: {self._current_mode} -> {output.current_mode}")
            self._current_mode = output.current_mode
            
            # Update property menu icon
            symbol = 'あ' if output.current_mode == pskk_pb2.HIRAGANA else 'A'
            self._prop_list.get(0).set_symbol(IBus.Text.new_from_string(symbol))
            self.update_property(self._prop_list.get(0))
            
            # Update radio button states
            for i in range(self._prop_list.get(0).get_sub_props().get_properties().__len__()):
                prop = self._prop_list.get(0).get_sub_props().get(i)
                if prop.get_key() == 'InputMode.Hiragana':
                    prop.set_state(IBus.PropState.CHECKED if output.current_mode == pskk_pb2.HIRAGANA else IBus.PropState.UNCHECKED)
                    self.update_property(prop)
                elif prop.get_key() == 'InputMode.Alphanumeric':
                    prop.set_state(IBus.PropState.CHECKED if output.current_mode == pskk_pb2.ALPHANUMERIC else IBus.PropState.UNCHECKED)
                    self.update_property(prop)
        
        # Handle commit
        if output.commit_string:
            logger.debug(f"Committing: {output.commit_string}")
            self.commit_text(IBus.Text.new_from_string(output.commit_string))
        
        # Update preedit
        if output.preedit_segments:
            preedit_text = ''.join(seg.text for seg in output.preedit_segments)
            logger.debug(f"Preedit: {preedit_text} (cursor: {output.preedit_cursor_pos})")
            
            # Create IBus.Text with attributes
            text = IBus.Text.new_from_string(preedit_text)
            
            # Add underline attributes for each segment
            attrs = IBus.AttrList()
            pos = 0
            for seg in output.preedit_segments:
                seg_len = len(seg.text)
                if seg.is_selected:
                    # Selected segment - reverse colors
                    attrs.append(IBus.Attribute.new(
                        IBus.AttrType.BACKGROUND,
                        0x000000,  # Black background
                        pos, pos + seg_len
                    ))
                    attrs.append(IBus.Attribute.new(
                        IBus.AttrType.FOREGROUND,
                        0xFFFFFF,  # White foreground
                        pos, pos + seg_len
                    ))
                else:
                    # Normal segment - underline
                    attrs.append(IBus.Attribute.new(
                        IBus.AttrType.UNDERLINE,
                        IBus.AttrUnderline.SINGLE,
                        pos, pos + seg_len
                    ))
                pos += seg_len
            
            text.set_attributes(attrs)
            
            self.update_preedit_text(text, output.preedit_cursor_pos, True)
        else:
            # Clear preedit
            self.hide_preedit_text()
        
        # Update candidates
        if output.show_candidates and output.candidates:
            logger.debug(f"Candidates: {len(output.candidates)} items")
            
            lookup_table = IBus.LookupTable.new(
                page_size=9,
                cursor_pos=output.candidate_cursor_pos,
                cursor_visible=True,
                round=True
            )
            
            for candidate in output.candidates:
                text = IBus.Text.new_from_string(candidate.surface)
                lookup_table.append_candidate(text)
            
            self.update_lookup_table(lookup_table, True)
        else:
            # Hide candidates
            self.hide_lookup_table()


def main():
    """Main entry point"""
    logger.info("Starting PSKK IBus Engine")
    
    # Initialize IBus
    IBus.init()
    
    bus = IBus.Bus()
    if not bus.is_connected():
        logger.error("Failed to connect to IBus daemon")
        sys.exit(1)
    
    # Create engine factory
    factory = IBus.Factory.new(bus.get_connection())
    factory.add_engine('pskk', PSKKEngine)
    
    # Register component
    component = IBus.Component.new(
        'net.kirameister.pskk',
        'PSKK',
        '0.1.0',
        'MIT',
        'Akira K.',
        'https://github.com/kirameister/pskk',
        '',
        'pskk'
    )
    
    engine_desc = IBus.EngineDesc.new(
        'pskk',
        'PSKK',
        'PSKK - Japanese Input Method',
        'ja',
        'MIT',
        'Akira K.',
        '',
        'default'
    )
    component.add_engine(engine_desc)
    
    bus.register_component(component)
    bus.request_name('net.kirameister.pskk', 0)
    
    logger.info("PSKK IBus Engine registered, entering main loop")
    
    # Run main loop
    main_loop = GLib.MainLoop()
    main_loop.run()


if __name__ == '__main__':
    main()
