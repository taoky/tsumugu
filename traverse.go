package main

import (
	"flag"
	"fmt"
	"io"
	"io/ioutil"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"sync/atomic"
	"time"
)

// StandardClient the default client that traverse uses.
var StandardClient = &http.Client{
	// Timeout: time.Second * 30,
}

var visited sync.Map
var boundaryPrefix string
var boundaryPrefixURL *url.URL
var boundaryHost string // same domain with different ports is acceptable here.
var dry = false
var excludeList []string // TODO: Implement exclusion when downloading

// File a simple struct representing local files
type File struct {
	name  string
	isDir bool
}

func get(url *url.URL) (*http.Response, *url.URL, error) {
	urlString := url.String()
	resp, err := StandardClient.Get(urlString)
	if err != nil {
		return nil, url, err
	}
	finalURL := resp.Request.URL
	err = isURLOutOfBoundary(finalURL)
	if err != nil {
		return nil, finalURL, err
	}
	finalURL = sanitizeURL(finalURL)
	return resp, resp.Request.URL, nil
}

func crawl(url *url.URL, queue chan *url.URL, baseFolder string) {
	fmt.Printf("Handling URL %s\n", url.String())
	if _, loaded := visited.LoadOrStore(url.String(), true); !loaded {
		resp, finalURL, err := get(url)
		if err != nil {
			log.Println(err)
			return
		}
		defer resp.Body.Close()

		fmt.Printf("%s: %d\n", finalURL.String(), resp.StatusCode)
		statusOK := resp.StatusCode >= 200 && resp.StatusCode < 300
		if !statusOK {
			log.Printf("URL %s got %d\n", finalURL.String(), resp.StatusCode)
		} else {
			if url.String() != finalURL.String() {
				fromPath := getFileRelPath(url) // the symlink
				toPath := getFileRelPath(finalURL)
				fromFullPath := filepath.Join(baseFolder, fromPath)
				toFullPath := filepath.Join(baseFolder, toPath)
				if fromFullPath != toFullPath {
					// create symlink
					// TODO: replace ln -sr with os.Symlink
					// TODO: change symlink when it is changed on remote
					if _, err := os.Stat(fromFullPath); os.IsNotExist(err) {
						cmd := exec.Command("gln", "-sr", toFullPath, fromFullPath)
						output, err := cmd.Output()
						fmt.Println(output)
						if err != nil {
							fmt.Printf("Create symlink %s -> %s failed: %v with (%s)\n", toFullPath, fromFullPath, err, err.(*exec.ExitError).Stderr)
						}
					}
				}
				queue <- finalURL
			} else if IsHTML(resp.Header) {
				folderRelPath := getFileRelPath(finalURL)
				folderPath := filepath.Join(baseFolder, folderRelPath)
				err = os.MkdirAll(folderPath, 0755)
				if err != nil {
					log.Printf("Create %s failed: %v\n", folderPath, err)
					return
				}
				hrefs := getHrefsFromHTML(resp.Body)

				remoteList := generateRemoteFileList(finalURL, hrefs)
				localFileInfoList, err := ioutil.ReadDir(folderPath)
				if err != nil {
					log.Printf("Error when reading folder %s: %v\n", folderPath, err)
					return
				}
				var localList []File
				for _, file := range localFileInfoList {
					localList = append(localList, File{filepath.Join(folderRelPath, file.Name()), file.IsDir()})
				}

				syncList, removeList := getSyncAndRemoveList(remoteList, localList)
				fmt.Println(remoteList, localList)
				fmt.Println(syncList, removeList)
				for _, name := range removeList {
					fullName := filepath.Join(folderPath, name)
					err = os.RemoveAll(fullName)
					if err != nil {
						log.Printf("Failed to remove old file %s: %v\n", fullName, err)
					} else {
						log.Printf("Old file %s successfully removed.\n", fullName)
					}
				}

				for _, href := range syncList {
					newURL, err := urlBuilder(boundaryPrefixURL, href)
					if err != nil {
						log.Printf("Failed when building URL %s with %s: %v\n", finalURL.String(), href, err)
					} else {
						log.Printf("Add %s to queue\n", newURL.String())
						queue <- newURL
					}
				}
			} else {
				downloadPath := getFileRelPath(finalURL)
				downloadPath = filepath.Join(baseFolder, downloadPath)
				if _, err := os.Stat(downloadPath); os.IsNotExist(err) {
					fmt.Printf("Downloading %s -> %s\n", finalURL.String(), downloadPath)

					out, err := ioutil.TempFile(filepath.Dir(downloadPath), filepath.Base(downloadPath))
					if err != nil {
						log.Printf("Create tmp file failed: %v\n", err)
						return
					}
					defer os.Remove(out.Name())

					if !dry {
						_, err = io.Copy(out, resp.Body)
						if err != nil {
							log.Println(err)
							return
						}
					} else {
						fmt.Println("Dry run (not actually downloading)")
					}

					err = os.Rename(out.Name(), downloadPath)
					if err != nil {
						log.Printf("Move %s -> %s failed: %v\n", out.Name(), downloadPath, err)
					}
				} else {
					fmt.Printf("%s exists.\n", downloadPath)
				}
				return
			}
		}
	} else {
		fmt.Printf("%s visited before.\n", url.String())
	}
}

func parseAndPush(targetString string, queue chan *url.URL, addTrailingSlash bool) error {
	target, err := url.Parse(targetString)
	if err != nil {
		return err
	}
	if addTrailingSlash {
		addTrailingSlashForAbsURL(target)
	}
	queue <- target
	return nil
}

func main() {
	bindIP := flag.String("bind", "", "The IP address that traverse binds to when downloading data.")
	workersNum := flag.Int("workers", 1, "The number of workers (goroutine for crawling)")
	flag.Parse()

	if *bindIP == "" {
		StandardClient = &http.Client{}
	} else {
		localAddr, err := net.ResolveIPAddr("ip", *bindIP)
		if err != nil {
			log.Fatal(err)
		}

		localTCPAddr := net.TCPAddr{
			IP: localAddr.IP,
		}
		StandardClient = &http.Client{
			Transport: &http.Transport{
				DialContext: (&net.Dialer{
					LocalAddr: &localTCPAddr,
				}).DialContext,
			},
		}
	}

	var queue = make(chan *url.URL, 1024)
	var tokens = make(chan struct{}, *workersNum)
	var cnt int64

	baseString := "https://download.docker.com/"
	// parseAndPush("https://download.docker.com/linux/static/", queue, true)
	// parseAndPush("https://download.docker.com/mac/static/", queue, true)
	parseAndPush("https://download.docker.com/win/static/", queue, true)

	base, err := url.Parse(baseString)
	if err != nil {
		log.Fatal(err)
	}

	validateEnterpoint(base)

	boundaryPrefix = base.Path
	boundaryPrefixURL = base
	boundaryHost = base.Hostname()

	if err != nil {
		log.Fatal(err)
	}
	for {
		select {
		case url := <-queue:
			atomic.AddInt64(&cnt, 1)
			go func() {
				tokens <- struct{}{}
				defer atomic.AddInt64(&cnt, -1)
				crawl(url, queue, "/tmp/test")
				<-tokens
			}()
		default:
			if atomic.LoadInt64(&cnt) == 0 {
				goto fin
			}
			time.Sleep(50 * time.Millisecond)
		}
		if getMemUsage() > (2 << 31) {
			// if larger than 4GB then kill self.
			log.Fatal("Eating too much memory (> 4GiB). This usually indicates that the website has TOO MANY links, or this program has a serious bug.")
		}
	}
fin:
	fmt.Println("Finished. Cleaning up...")
}
