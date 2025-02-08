#!/bin/bash

# 示例脚本：演示如何使用 curl 直接调用 API
# 使用方法：./example.sh "你的问题"

# 配置
API_TOKEN="your-token-here"  # 替换为您的 API 令牌
MODEL="deepseek-r1"          # 可选模型之一
API_URL="https://qianfan.baidubce.com/v2/chat/completions"

# 检查参数
if [ -z "$1" ]; then
    echo "使用方法: $0 \"你的问题\""
    exit 1
fi

# 发送请求
curl -X POST "$API_URL" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $API_TOKEN" \
    -d "{
        \"model\": \"$MODEL\",
        \"messages\": [
            {
                \"role\": \"user\",
                \"content\": \"$1\"
            }
        ]
    }"

echo -e "\n"



