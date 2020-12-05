package main

import (
	"fmt"
	"io/ioutil"
	"log"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"sync/atomic"
	"time"
)

var StandardClient = &http.Client{
	Timeout: time.Second * 30,
}

var visited sync.Map
var boundaryPrefix string
var boundaryHost string // same domain with different ports is acceptable here.

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
				if fromPath != toPath {
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
				folderPath := getFileRelPath(finalURL)
				folderPath = filepath.Join(baseFolder, folderPath)
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
				var localList []string
				for _, file := range localFileInfoList {
					localList = append(localList, file.Name())
				}

				_, removeList := getSyncAndRemoveList(remoteList, localList)
				fmt.Println(removeList)
				for _, name := range removeList {
					fullName := filepath.Join(folderPath, name)
					err = os.Remove(fullName)
					if err != nil {
						log.Printf("Failed to remove old file %s: %v\n", fullName, err)
					} else {
						log.Printf("Old file %s successfully removed.\n", fullName)
					}
				}

				for _, href := range hrefs {
					newURL, err := urlBuilder(finalURL, href)
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
					fmt.Println("Dry run (not actually downloading)")

					out, err := ioutil.TempFile(filepath.Dir(downloadPath), filepath.Base(downloadPath))
					if err != nil {
						log.Printf("Create tmp file failed: %v\n", err)
						return
					}
					defer os.Remove(out.Name())

					/* _, err = io.Copy(out, resp.Body)
					if err != nil {
						log.Println(err)
						return
					} */
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
	workersNum := 5
	var queue = make(chan *url.URL, workersNum)
	var cnt int64

	baseString := "https://download.docker.com/"
	parseAndPush("https://download.docker.com/linux/static/", queue, true)
	parseAndPush("https://download.docker.com/mac/static/", queue, true)
	parseAndPush("https://download.docker.com/win/static/", queue, true)

	base, err := url.Parse(baseString)
	if err != nil {
		log.Fatal(err)
	}

	validateEnterpoint(base)

	boundaryPrefix = base.Path
	boundaryHost = base.Hostname()

	if err != nil {
		log.Fatal(err)
	}
	for {
		select {
		case url := <-queue:

			atomic.AddInt64(&cnt, 1)
			go func() {
				defer atomic.AddInt64(&cnt, -1)
				crawl(url, queue, "/tmp/test")
			}()
		default:
			if atomic.LoadInt64(&cnt) == 0 {
				goto fin
			}
		}
	}
fin:
	fmt.Println("Finished. Cleaning up...")
}
