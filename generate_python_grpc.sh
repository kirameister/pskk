#!/bin/bash
# Generate Python gRPC stubs from proto file

set -e

echo "Generating Python gRPC stubs..."

# Install grpcio-tools if not already installed
if ! python3 -c "import grpc_tools" 2>/dev/null; then
    echo "Installing grpcio-tools..."
    pip3 install --user grpcio-tools
fi

# Generate stubs
python3 -m grpc_tools.protoc \
    -I./proto \
    --python_out=./proto \
    --grpc_python_out=./proto \
    ./proto/pskk.proto

echo "✓ Generated proto/pskk_pb2.py"
echo "✓ Generated proto/pskk_pb2_grpc.py"
echo ""
echo "Python gRPC stubs generated successfully!"
