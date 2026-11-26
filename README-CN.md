<div align="center">
<img src="https://github.com/quackduck/devzat/assets/38882631/046fbb4d-dff2-41e9-a61c-271d0820473e" style="height: 100px; border-radius: 50px;" />
</div>

***

<a href="https://www.producthunt.com/posts/devzat?utm_source=badge-top-post-badge&utm_medium=badge&utm_souce=badge-devzat" target="_blank"><img src="https://api.producthunt.com/widgets/embed-image/v1/top-post-badge.svg?post_id=298678&theme=light&period=daily" alt="Devzat - Chat with other devs over SSH in your Terminal | Product Hunt" style="width: 250px; height: 54px;" width="250" height="54" /></a>

开发人员在哪里？Devzat！

Devzat是一个自定义SSH服务器，它能将你带入一个聊天室而非shell prompt。由于所有平台（甚至手机）上都有 SSH 应用程序，因此你可以在任何设备上连接到 Devzat！


<!-- <img src="https://user-images.githubusercontent.com/38882631/115499526-a4d70280-a280-11eb-8723-817f54eccf3e.png" height=400px /> -->

这是有一天录制的记录:
[![asciicast](https://asciinema.org/a/477083.svg)](https://asciinema.org/a/477083?speed=3)
## 使用方法

试试看:

```sh
ssh devzat.hackclub.com
```

如果这是第一次登录，可以使用 SSH 用户名选择显示名称。例如，如果您想被称为 “wenjie”，可以运行：
```sh
ssh wenjie@devzat.hackclub.com
```
如果想在首次登录后更改显示名称，应在登录后使用 `nick` 命令。


如果您在防火墙下，您仍然可以通过端口 443 加入：
```sh
ssh devzat.hackclub.com -p 443
```

如果将其添加到 `~/.ssh/config`：
```ssh
Host chat
    HostName devzat.hackclub.com
```

您只需：
```sh
ssh chat
```

我们还有一个 Slack 桥！如果你在 [Hack Club](https://hackclub.com) Slack 上，请查看 `#ssh-chat-bridge` 频道！

如果遇到问题，请随时提交 [新issue](https://github.com/quackduck/devzat/issues)。

查看Devzat主服务器的 [站点状态](https://stats.uptimerobot.com/kxMQqfYk4y) 以检查检查是否可能出现故障。


### 想要托管自己的实例？

快速开始:
```shell
git clone https://github.com/quackduck/devzat && cd devzat
go install # or build, if you want to keep things pwd
ssh-keygen -qN '' -f devzat-sshkey # new ssh host key for the server
devzat # run! the default config is used & written automatically
```
这些命令用于下载、构建、设置和运行 Devzat 服务器，默认端口为 2221（可通过设置 `$PORT` 更改）。

查看[Admin's Manual](Admin's%20Manual.md)，了解完整的自托管文档！

### 拒绝权限？

Devzat 使用公钥来识别用户。如果您被拒绝访问：`foo@devzat.hackclub.com: Permission denied (publickey)`， 尝试登录无需密钥的 **443** 端口。
`ssh devzat.hackclub.com -p 443`



### 帮助

```text
注：聊天室中输入Help获取的是英文原始文本

欢迎来到Devzat！Devzat通过SSH聊天：github.com/quackduck/devzat
由于所有平台上，包括移动设备上都有 SSH 应用，你可以从任何地方加入。

运行 `cmds` 查看命令列表。

有趣的功能:
• 房间！运行 cd 查看所有房间，使用 cd #foo 加入新房间。
• 支持 Markdown！表格、标题、斜体等一切。只需用 \n 代替换行符即可。
• 代码语法高亮 使用 Markdown fences发送代码。运行 eg-code 查看示例。
• 私聊！使用 =user <msg> 发送快速 DM，或通过运行 cd @user 留在 DM 中。
• 支持时区，使用 tz Continent（州）/City（城市）设置时区。

• 内置Tic Tan Toe（五子棋）和Hangman (猜单词）！运行 tic 或者 hang<word> 来开始新游戏
• emoji 替换！:rocket: => 🚀 （就像在 Slack 和 Discord 上一样）

在替换换行符时，我经常使用 bulkseotools.com/add-remove-line-breaks.php。

由 Ishan Goel 用朋友们的创意制作而成。
感谢 Caleb Denio 借出他的服务器！
```
### 指令
```text
注：聊天室中输入cmds/rest获取的是英文原始文本
Commands
   =<user>   <msg>           向 <user> 发送私聊信息 <msg>
   users                     列出用户
   color     <color>         改变名字颜色
   exit                      离开聊天室
   help                      展示帮助信息
   man       <cmd>           获取特定命令帮助
   emojis                    查看emojis列表
   bell      on|off|all      ANSI铃声开启(on)/从不（off）/每条消息均响（all）
   clear                     清屏
   hang      <char|word>     玩 hangman
   tic       <cell num>      玩 tic tac toe!
   devmonk                   测试打字速度
   cd        #room|user      加入 #room，私聊用户或运行 cd 查看列表
   tz        <zone> [24h]    设置您的 IANA 时区（例如 tz Asia/Dubai），并可选择设置 24h
   nick      <name>          改变用户名
   theme     <theme>|list    更改语法高亮主题
   rest                      不常用的命令列表 
   cmds                      展示此命令
```
```
The rest
   people                  查看加入的人的信息
   id       <user>         获取用户的唯一ID(hashed key)
   admins                  打印所有管理员的 ID(hashed key)
   eg-code  [big]          语法高亮代码示例
   lsbans                  被禁言的 ID 列表
   ban      <user>         禁言 <user> (admin)
   unban    <IP|ID> [dur]  解除对某人的禁言，可选择持续时间（admin）
   kick     <user>         踢出 <user>登录 (admin)
   art                     展示一些熊猫的图
   pwd                     展示你的当前房间
   shrug                   ¯\_(ツ)_/¯
```
提示：如果昵称因网络延迟而被占用，`kick` 可以帮助踢出之前昵称。

## 集成

在自托管实例中，Devzat 可与 Slack 和/或 Discord 集成以桥接消息，并与 Twitter 集成以发布新用户公告。
请参阅 [Admin's Manual](Admin's%20Manual.md) 获得更多信息。


Devzat 拥有一个插件 API，您可以用它来集成自己的服务： [documentation](plugin/README.md)。
您可以随意在主实例中添加插件。只需在服务器上申请一个 token 即可。




## 星标历史

[![Stargazers over time](https://starchart.cc/quackduck/devzat.svg)](https://starchart.cc/quackduck/devzat)


### 参与者

您可能认识的人加入者：

Zach Latta - Founder of Hack Club: _"omg amazing! this is so awesome"_  
Ant Wilson - Co founder, Supabase: [_"brilliant!"_](https://twitter.com/AntWilson/status/1396444302721445889)  
Bereket [@heybereket](https://twitter.com/heybereket): _"this is pretty cool"_  
Ayush [@ayshptk](https://twitter.com/ayshptk): _"Can I double star the repo somehow :pleading_face:"_  
Sanketh [@SankethYS](https://twitter.com/SankethYS): _"Heck! How does this work. So cool."_  
Tony Dinh [@tdinh_me](https://twitter.com/tdinh_me): _"supeer cool, oh, open source as well? yeah"_  
Srushti [@srushtiuniverse](https://twitter.com/srushtiuniverse): _"Yess it's awesome. I tried it."_  
Surjith [@surjithctly](https://twitter.com/surjithctly): _"Whoa, who made this?"_  
Arav [@HeyArav](https://twitter.com/HeyArav): [_"Okay, this is actually super awesome."_](https://twitter.com/tregsthedev/status/1384180393893498880)  
Harsh [@harshb__](https://twitter.com/harshb__): _"im gonna come here everyday to chill when i get bored of studying lol, this is so cool"_
Krish [@krishnerkar_](https://twitter.com/krishnerkar_):  [_"SHIT! THIS IS SO DOPE"_](https://twitter.com/krishnerkar_/status/1384173042616573960)  
Amrit [@astro_shenava](https://twitter.com/astro_shenava): _"Super cool man"_  
Mudrank [@mudrankgupta](https://twitter.com/mudrankgupta): "🔥🚀🚀"

来自 Hack Club:  
**[Caleb Denio](https://calebden.io), [Safin Singh](https://safin.dev), [Eleeza](https://github.com/E-Lee-Za)   
[Jubril](https://github.com/s1ntaxe770r), [Sarthak Mohanty](https://sarthakmohanty.me)    
[Sam Poder](http://sampoder.com), [Rishi Kothari](http://rishi.cx)    
[Amogh Chaubey](https://amogh.sh), [Ella](https://ella.cx/), [Hugo Hu](https://github.com/Hugoyhu)
[Matthew Stanciu](https://matthewstanciu.me/), [Tanishq Soni](https://tanishqsoni.me)**

非常感谢了不起的 [Caleb Denio](https://github.com/cjdenio)借给我最初的 Devzat 服务器 💖


### *由 [Ishan Goel](https://twitter.com/usrbinishan/) 根据朋友的特色想法制作。感谢 [Caleb Denio](https://twitter.com/CalebDenio)借出他的服务器！*