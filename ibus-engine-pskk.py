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
from pathlib import Path

# Add proto directory to path
proto_dir = Path(__file__).parent / 'proto'
sys.path.insert(0, str(proto_dir))

# Import generated gRPC stubs (we'll generate these)
try:
    import pskk_pb2
    import pskk_pb2_grpc
except ImportError:
    print("Error: gRPC stubs not found. Run: python -m grpc_tools.protoc -I./proto --python_out=./proto --grpc_python_out=./proto ./proto/pskk.proto")
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
        
        # Connect to gRPC server
        self.channel = None
        self.stub = None
        self.connect_to_server()
        
        # Property list for the input mode menu
        self._prop_list = self._create_properties()
        
    def connect_to_server(self):
        """Connect to the PSKK gRPC server"""
        try:
            self.channel = grpc.insecure_channel('localhost:50051')
            self.stub = pskk_pb2_grpc.PSKKServiceStub(self.channel)
            logger.info("Connected to PSKK gRPC server at localhost:50051")
        except Exception as e:
            logger.error(f"Failed to connect to PSKK gRPC server: {e}")
            logger.error("Make sure pskk-server is running: cargo run --bin pskk-server")
    
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
        
        if prop_name == 'InputMode.Hiragana':
            self._set_mode(pskk_pb2.HIRAGANA)
        elif prop_name == 'InputMode.Alphanumeric':
            self._set_mode(pskk_pb2.ALPHANUMERIC)
        elif prop_name == 'Settings':
            self._open_settings()
    
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
        import subprocess
        try:
            subprocess.Popen(['pskk-settings'])
        except Exception as e:
            logger.error(f"Failed to open settings: {e}")
    
    def do_process_key_event(self, keyval, keycode, state):
        """
        Main key event handler - forwards to gRPC server
        Returns True if the key was handled, False otherwise
        """
        # Convert IBus key event to PSKK KeyEvent
        key_char = chr(keyval) if 32 <= keyval < 127 else ""
        key_name = IBus.keyval_name(keyval) or str(keyval)
        is_pressed = True  # IBus only sends press events
        
        # Modifier keys
        shift = bool(state & IBus.ModifierType.SHIFT_MASK)
        ctrl = bool(state & IBus.ModifierType.CONTROL_MASK)
        alt = bool(state & IBus.ModifierType.MOD1_MASK)
        
        logger.debug(f"Key: {key_name} (char={key_char}, shift={shift}, ctrl={ctrl}, alt={alt})")
        
        if not self.stub:
            logger.warning("No gRPC connection, key not processed")
            return False
        
        try:
            # Create gRPC request
            modifiers = pskk_pb2.KeyModifiers(
                shift=shift,
                ctrl=ctrl,
                alt=alt
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
            pos = 0
            for seg in output.preedit_segments:
                seg_len = len(seg.text)
                if seg.is_selected:
                    # Selected segment - reverse colors
                    attr = IBus.Attribute.new(
                        IBus.AttrType.BACKGROUND,
                        0x000000,  # Black background
                        pos, pos + seg_len
                    )
                    text.append_attribute(attr)
                    attr = IBus.Attribute.new(
                        IBus.AttrType.FOREGROUND,
                        0xFFFFFF,  # White foreground
                        pos, pos + seg_len
                    )
                    text.append_attribute(attr)
                else:
                    # Normal segment - underline
                    attr = IBus.Attribute.new(
                        IBus.AttrType.UNDERLINE,
                        IBus.AttrUnderline.SINGLE,
                        pos, pos + seg_len
                    )
                    text.append_attribute(attr)
                pos += seg_len
            
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
